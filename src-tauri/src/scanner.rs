//! Core scanner: walks a root folder, finds project roots, and computes
//! size / junk / activity metadata for each. Every surfaced item is classified
//! (project, container, cache, application data, archive, file, …) so caches and
//! application data never inflate the project counts.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use walkdir::WalkDir;

use crate::classify::{self, Signals};
use crate::detector;
use crate::health;
use crate::junk;
use crate::model::{ItemType, JunkEntry, Project};

/// Build artifact categories (everything recoverable that isn't node_modules,
/// archives or logs) — used for the legacy `build_bytes` rollup.
const BUILD_CATS: &[&str] = &[
    ".next", "dist", "build", "coverage", "target", "bin", "obj", "__pycache__", ".venv",
];

/// Normalize a path for cross-platform set comparison (lowercase, `\` seps,
/// no trailing separator).
fn norm(path: &Path) -> String {
    path.to_string_lossy()
        .to_lowercase()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_string()
}

/// Scan `root` recursively and return every detected item.
pub fn scan(root: &Path) -> Vec<Project> {
    scan_excluding(root, &[])
}

/// Scan `root`, skipping any sub-directory that is itself a registered workspace
/// (`excludes`). This prevents an outer workspace (e.g. `C:\Users\me`) from
/// stealing the projects of a nested one (e.g. `…\Downloads`).
pub fn scan_excluding(root: &Path, excludes: &[PathBuf]) -> Vec<Project> {
    let now = now_secs();
    let mut items: Vec<Project> = Vec::new();
    let skip: HashSet<String> = excludes
        .iter()
        .map(|p| norm(p))
        .filter(|s| s != &norm(root)) // never skip the root we're scanning
        .collect();

    // The workspace root is authority. Collapse it into a single project ONLY
    // when it genuinely is one: it has its own .git, or it contains no child
    // project roots. A folder like Downloads that merely has a stray marker
    // (a downloaded package.json / Dockerfile) but holds real sub-projects is a
    // workspace to scan, not one giant project.
    if detector::is_project_root(root)
        && (root.join(".git").exists() || child_project_roots(root).is_empty())
    {
        if let Some(p) = analyze_project(root, now, &[]) {
            items.push(p);
        }
        return items;
    }

    // Root level: dirs are traversed; loose files become Archive/File items so a
    // real dev workspace like Downloads reports its clutter too.
    let mut stack: Vec<PathBuf> = Vec::new();
    visit_root(root, &mut stack, &mut items, now, &skip);

    while let Some(dir) = stack.pop() {
        // Location-based classification (cache / store / appdata / system). A
        // strong project signal at the directory overrides its location (§5).
        if let Some(t) = classify::path_noise(&dir) {
            if !detector::is_project_root(&dir) {
                items.push(leaf_item(&dir, t));
                continue; // never descend into caches / application data
            }
        }

        let child_roots = child_project_roots(&dir);

        // A project root collapses into one project when it owns .git (monorepo)
        // or has no sub-projects. If it has a stray marker but real sub-projects,
        // fall through so those surface individually.
        if detector::is_project_root(&dir)
            && (dir.join(".git").exists() || child_roots.is_empty())
        {
            if let Some(p) = analyze_project(&dir, now, &[]) {
                items.push(p);
            }
            continue;
        }

        // A folder holding ≥2 independent project roots (none with their own
        // .git) is a container of projects, analyzed as one grouped item.
        let groupable = child_roots.len() >= 2 && child_roots.iter().all(|c| !c.join(".git").exists());
        if groupable {
            if let Some(p) = analyze_project(&dir, now, &child_roots) {
                items.push(p);
            }
            continue;
        }

        push_children(&dir, &mut stack, &skip);
    }

    items
}

/// Enumerate the workspace root: push child dirs, emit loose files as items.
fn visit_root(
    root: &Path,
    stack: &mut Vec<PathBuf>,
    items: &mut Vec<Project>,
    now: i64,
    skip: &HashSet<String>,
) {
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if junk::is_junk_dir(&name)
                || detector::is_skip_dir(&name)
                || name.starts_with('.')
                || skip.contains(&norm(&path))
            {
                continue;
            }
            stack.push(path);
        } else if path.is_file() && !name.starts_with('.') && !is_system_file(&name) {
            items.push(file_item(&path, now));
        }
    }
}

fn is_system_file(name: &str) -> bool {
    let l = name.to_lowercase();
    matches!(l.as_str(), "desktop.ini" | "thumbs.db" | "ntuser.dat" | "ntuser.ini")
}

fn push_children(dir: &Path, stack: &mut Vec<PathBuf>, skip: &HashSet<String>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if junk::is_junk_dir(&name)
                || detector::is_skip_dir(&name)
                || name.starts_with('.')
                || skip.contains(&norm(&path))
            {
                continue;
            }
            stack.push(path);
        }
    }
}

fn child_project_roots(dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if junk::is_junk_dir(&name) || detector::is_skip_dir(&name) || name.starts_with('.') {
                continue;
            }
            if detector::is_project_root(&path) {
                roots.push(path);
            }
        }
    }
    roots
}

fn base_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

/// A location-classified directory (cache / store / appdata / system). Recorded
/// cheaply — no deep walk — since it never counts as a project.
fn leaf_item(dir: &Path, item_type: ItemType) -> Project {
    empty_item(dir, item_type)
}

/// A loose file at the workspace root, classified by extension.
fn file_item(path: &Path, _now: i64) -> Project {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut p = empty_item(path, classify::file_type(path));
    p.size_bytes = size;
    p
}

fn empty_item(path: &Path, item_type: ItemType) -> Project {
    Project {
        id: 0,
        path: path.to_string_lossy().to_string(),
        name: base_name(path),
        item_type,
        stack: Vec::new(),
        size_bytes: 0,
        junk_bytes: 0,
        node_modules_bytes: 0,
        build_bytes: 0,
        archive_bytes: 0,
        junk_detail: Vec::new(),
        git_present: false,
        has_readme: false,
        last_activity: 0,
        health_score: 0,
        confidence: 0,
        workspace_id: None,
        ignored: false,
    }
}

/// Classify a file (by its path relative to the project) into a junk category,
/// or None if it is a normal source file.
fn junk_category(rel: &Path) -> Option<String> {
    for comp in rel.components() {
        if let Component::Normal(os) = comp {
            let name = os.to_string_lossy();
            if let Some(cat) = junk::category_for_dir(&name) {
                return Some(cat.to_string());
            }
        }
    }
    if let Some(ext) = rel.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        if junk::is_archive_ext(&ext) {
            return Some("archives".to_string());
        }
        if ext == "log" {
            return Some("logs".to_string());
        }
    }
    None
}

fn analyze_project(dir: &Path, now: i64, child_roots: &[PathBuf]) -> Option<Project> {
    let name = base_name(dir);

    let mut size_bytes: u64 = 0;
    let mut detail: HashMap<String, u64> = HashMap::new();
    let mut last_activity: i64 = 0;

    for entry in WalkDir::new(dir).follow_links(false).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let len = meta.len();
        size_bytes += len;

        let rel = entry.path().strip_prefix(dir).unwrap_or(entry.path());
        match junk_category(rel) {
            Some(cat) => {
                *detail.entry(cat).or_insert(0) += len;
            }
            None => {
                if let Ok(modified) = meta.modified() {
                    let secs = system_time_secs(modified);
                    if secs > last_activity {
                        last_activity = secs;
                    }
                }
            }
        }
    }

    let junk_bytes: u64 = detail.values().sum();
    let node_modules_bytes = detail.get("node_modules").copied().unwrap_or(0);
    let archive_bytes = detail.get("archives").copied().unwrap_or(0);
    let build_bytes: u64 = BUILD_CATS
        .iter()
        .map(|c| detail.get(*c).copied().unwrap_or(0))
        .sum();

    let mut junk_detail: Vec<JunkEntry> = detail
        .into_iter()
        .map(|(name, bytes)| JunkEntry { name, bytes })
        .collect();
    junk_detail.sort_by(|a, b| b.bytes.cmp(&a.bytes));

    let git_present = dir.join(".git").exists();
    let has_readme = detector::has_readme(dir);
    let is_wordpress = detector::is_wordpress(dir);
    let grouped = !child_roots.is_empty();

    let signals = Signals {
        git: git_present,
        manifest: detector::has_manifest(dir),
        readme: has_readme,
        config: detector::has_config(dir),
        source_dir: detector::has_source_dir(dir),
        wordpress: is_wordpress,
    };
    let (item_type, confidence) = classify::classify_project(signals, grouped);

    let mut stack = detector::detect_stack(dir);
    if grouped {
        for c in child_roots {
            for t in detector::detect_stack(c) {
                if t != "Git" && !stack.iter().any(|x| x == &t) {
                    stack.push(t);
                }
            }
        }
        if !stack.iter().any(|x| x.as_str() == "Monorepo") {
            stack.insert(0, "Monorepo".to_string());
        }
    }

    let mut project = Project {
        id: 0,
        path: dir.to_string_lossy().to_string(),
        name,
        item_type,
        stack,
        size_bytes,
        junk_bytes,
        node_modules_bytes,
        build_bytes,
        archive_bytes,
        junk_detail,
        git_present,
        has_readme,
        last_activity,
        health_score: 0,
        confidence,
        workspace_id: None,
        ignored: false,
    };
    project.health_score = health::compute(&project, now);
    Some(project)
}

fn now_secs() -> i64 {
    system_time_secs(SystemTime::now())
}

fn system_time_secs(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::scan;
    use crate::model::ItemType;
    use std::fs;
    use std::path::PathBuf;

    fn tmp(name: &str) -> PathBuf {
        // Under the crate's target dir, NOT the OS temp — the OS temp lives in
        // AppData\Local\Temp, which the classifier (correctly) treats as cache.
        let d = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("scantests")
            .join(format!(
                "rnz_scan_{}_{}",
                name,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn count(items: &[crate::model::Project], t: ItemType) -> usize {
        items.iter().filter(|i| i.item_type == t).count()
    }

    #[test]
    fn stray_marker_workspace_is_not_collapsed() {
        // A Downloads-like folder: a stray manifest at the top level, but it holds
        // a real sub-project and a loose archive. It must NOT collapse to 1 project.
        let root = tmp("downloads");
        fs::write(root.join("package.json"), "{}").unwrap(); // stray downloaded file
        fs::write(root.join("random.zip"), "x").unwrap();
        let proj = root.join("client-a");
        fs::create_dir_all(&proj).unwrap();
        fs::write(proj.join("pyproject.toml"), "").unwrap();
        fs::create_dir_all(proj.join("src")).unwrap();

        let items = scan(&root);
        assert!(count(&items, ItemType::Project) >= 1, "sub-project must surface");
        assert!(
            items.iter().any(|i| i.name == "client-a" && i.item_type == ItemType::Project),
            "client-a should be a project"
        );
        assert_eq!(count(&items, ItemType::Archive), 1, "random.zip is an archive");
        // The stray package.json becomes a loose File, not a project root swallowing all.
        assert!(items.iter().any(|i| i.name == "package.json" && i.item_type == ItemType::File));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn nested_registered_workspace_is_skipped() {
        // Scanning an outer workspace must not descend into a nested one, so it
        // can't steal the nested workspace's projects.
        let root = tmp("outer");
        let inner = root.join("Downloads");
        fs::create_dir_all(&inner).unwrap();
        let proj = inner.join("client-a");
        fs::create_dir_all(&proj).unwrap();
        fs::write(proj.join("package.json"), "{}").unwrap();

        let all = super::scan(&root);
        assert!(all.iter().any(|i| i.name == "client-a"), "found without exclude");

        let excluded = super::scan_excluding(&root, &[inner.clone()]);
        assert!(
            !excluded.iter().any(|i| i.path.to_lowercase().contains("downloads")),
            "nested workspace must be skipped entirely"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn single_project_folder_collapses_to_one() {
        // A real no-git project registered directly: one manifest, a src dir, no
        // sub-projects → exactly one Project.
        let root = tmp("myproj");
        fs::write(root.join("package.json"), "{}").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src").join("index.js"), "").unwrap();

        let items = scan(&root);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_type, ItemType::Project);

        let _ = fs::remove_dir_all(&root);
    }
}
