//! Core scanner: walks a root folder, finds project roots, and computes
//! size / junk / activity metadata for each.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use walkdir::WalkDir;

use crate::detector;
use crate::health;
use crate::junk;
use crate::model::{JunkEntry, Project};

/// Build artifact categories (everything recoverable that isn't node_modules,
/// archives or logs) — used for the legacy `build_bytes` rollup.
const BUILD_CATS: &[&str] = &[
    ".next", "dist", "build", "coverage", "target", "bin", "obj", "__pycache__", ".venv",
];

/// Scan `root` recursively and return every detected project.
pub fn scan(root: &Path) -> Vec<Project> {
    let now = now_secs();
    let mut projects: Vec<Project> = Vec::new();

    if detector::is_project_root(root) {
        if let Some(p) = analyze_project(root, now, &[]) {
            projects.push(p);
        }
        return projects;
    }

    let mut stack: Vec<PathBuf> = Vec::new();
    push_children(root, &mut stack);

    while let Some(dir) = stack.pop() {
        if detector::is_project_root(&dir) {
            if let Some(p) = analyze_project(&dir, now, &[]) {
                projects.push(p);
            }
            continue;
        }

        let child_roots = child_project_roots(&dir);
        let groupable = child_roots.len() >= 2
            && child_roots.iter().all(|c| !c.join(".git").exists());
        if groupable {
            if let Some(p) = analyze_project(&dir, now, &child_roots) {
                projects.push(p);
            }
            continue;
        }

        push_children(&dir, &mut stack);
    }

    projects
}

fn push_children(dir: &Path, stack: &mut Vec<PathBuf>) {
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

fn confidence_for(git: bool, manifest: bool, readme: bool, wordpress: bool, grouped: bool) -> i32 {
    let mut c = 0;
    if git {
        c += 45;
    }
    if manifest {
        c += 35;
    }
    if readme {
        c += 20;
    }
    if wordpress {
        c += 55;
    }
    if grouped {
        c += 40;
    }
    c.min(100)
}

/// Classify a file (by its path relative to the project) into a junk category,
/// or None if it is a normal source file.
fn junk_category(rel: &Path) -> Option<String> {
    // The outermost junk directory in the path wins.
    for comp in rel.components() {
        if let Component::Normal(os) = comp {
            let name = os.to_string_lossy();
            if let Some(cat) = junk::category_for_dir(&name) {
                return Some(cat.to_string());
            }
        }
    }
    // Not inside a junk dir: classify the file itself.
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
    let name = dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| dir.to_string_lossy().to_string());

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
    let has_manifest = detector::has_manifest(dir);
    let is_wordpress = detector::is_wordpress(dir);
    let grouped = !child_roots.is_empty();

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
        confidence: confidence_for(git_present, has_manifest, has_readme, is_wordpress, grouped),
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
