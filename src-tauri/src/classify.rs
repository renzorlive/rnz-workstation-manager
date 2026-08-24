//! Item-level classification. Decides what a *discovered* directory/file is
//! (real project, cache, dependency store, application data, …) based on
//! evidence — never on the workspace's own name/path. A folder under Downloads
//! is judged the same way as one under E:\Projects.

use std::path::{Component, Path};

use crate::model::ItemType;

/// Lower-cased path components (drive/prefix stripped).
fn comps(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect()
}

/// Whether `comps` contains `seq` as a contiguous run of segments.
fn contains_seq(comps: &[String], seq: &[&str]) -> bool {
    if seq.is_empty() || seq.len() > comps.len() {
        return false;
    }
    comps
        .windows(seq.len())
        .any(|w| w.iter().zip(seq).all(|(a, b)| a == b))
}

/// Known dependency stores / caches, matched anywhere in the path as a run of
/// segments. Scoped and extensible — add rows, don't special-case workspaces.
const NOISE_SEQ: &[(&[&str], ItemType)] = &[
    (&[".cargo", "registry"], ItemType::DependencyStore),
    (&[".cargo", "git"], ItemType::DependencyStore),
    (&["go", "pkg", "mod"], ItemType::DependencyStore),
    (&[".nuget", "packages"], ItemType::DependencyStore),
    (&[".m2", "repository"], ItemType::DependencyStore),
    (&[".gradle", "caches"], ItemType::DependencyStore),
    (&[".yarn", "berry", "cache"], ItemType::DependencyStore),
    (&[".yarn", "cache"], ItemType::DependencyStore),
    (&[".pub-cache"], ItemType::DependencyStore),
    (&["pnpm-store"], ItemType::DependencyStore),
    (&[".pnpm-store"], ItemType::DependencyStore),
    (&[".rustup", "toolchains"], ItemType::DependencyStore),
    (&[".npm", "_cacache"], ItemType::Cache),
    (&[".npm", "_npx"], ItemType::Cache),
    (&[".cache"], ItemType::Cache),
    (&[".ollama", "models"], ItemType::DependencyStore),
];

/// System locations to skip when a whole drive / user profile is a workspace.
const SYSTEM_SEQ: &[(&[&str], ItemType)] = &[
    (&["windows"], ItemType::SystemData),
    (&["program files"], ItemType::SystemData),
    (&["program files (x86)"], ItemType::SystemData),
    (&["programdata"], ItemType::SystemData),
    (&["$recycle.bin"], ItemType::SystemData),
    (&["system volume information"], ItemType::SystemData),
];

/// Classify a directory purely by its *location*. Returns `Some` for known
/// cache/store/appdata/system locations (which should be recorded as a leaf and
/// NOT traversed), or `None` for a normal directory that should be scanned.
///
/// A strong project signal at the directory itself still overrides this — the
/// caller checks `detector::is_project_root` first (see §5: a real project in
/// AppData must remain detectable).
pub fn path_noise(path: &Path) -> Option<ItemType> {
    let c = comps(path);

    // AppData: the AppData/Local/Roaming/LocalLow dirs are containers to walk;
    // one level below them, everything is application data / cache / store.
    if let Some(i) = c.iter().position(|s| s == "appdata") {
        let after = &c[i + 1..];
        if after.len() >= 2 {
            return Some(appdata_leaf_type(after));
        }
        // AppData or AppData\<Local|Roaming|…> → traverse into it.
        return None;
    }

    for (seq, t) in NOISE_SEQ {
        if contains_seq(&c, seq) {
            return Some(*t);
        }
    }
    for (seq, t) in SYSTEM_SEQ {
        if contains_seq(&c, seq) {
            return Some(*t);
        }
    }
    None
}

/// Classify a leaf directly under AppData\<Local|Roaming|LocalLow>.
fn appdata_leaf_type(after: &[String]) -> ItemType {
    // after[0] = local/roaming/locallow/temp; after[1] = the leaf folder.
    let tail = &after[1..];
    if tail.iter().any(|s| s.contains("cache")) {
        return ItemType::Cache;
    }
    match after[1].as_str() {
        "pnpm" | "yarn" => ItemType::DependencyStore,
        "temp" | "_npx" => ItemType::Cache,
        _ => ItemType::ApplicationData,
    }
}

/// Classify a loose file by extension.
pub fn file_type(path: &Path) -> ItemType {
    const ARCHIVE: &[&str] = &[
        "zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "xz", "iso", "dmg",
    ];
    match path.extension().map(|e| e.to_string_lossy().to_lowercase()) {
        Some(ext) if ARCHIVE.contains(&ext.as_str()) => ItemType::Archive,
        _ => ItemType::File,
    }
}

/// Evidence gathered at a directory's top level for project scoring.
#[derive(Default, Clone, Copy)]
pub struct Signals {
    pub git: bool,
    pub manifest: bool,
    pub readme: bool,
    pub config: bool,
    pub source_dir: bool,
    pub wordpress: bool,
}

/// Turn signals into (ItemType, confidence 0-100). Grouped = a container of ≥2
/// child project roots (see scanner). No-Git is NOT disqualifying (§10).
pub fn classify_project(s: Signals, grouped: bool) -> (ItemType, i32) {
    if grouped {
        return (ItemType::ProjectContainer, 90);
    }
    let mut c = 0;
    if s.git {
        c += 45;
    }
    if s.wordpress {
        c += 55;
    }
    if s.manifest {
        c += 35;
    }
    if s.source_dir {
        c += 15;
    }
    if s.config {
        c += 10;
    }
    if s.readme {
        c += 10;
    }
    let confidence = c.min(100);

    // Evidence-based type: a manifest / git / wordpress / (readme+src) is a
    // real project; otherwise it's unclassified.
    let is_project = s.git || s.manifest || s.wordpress || (s.readme && s.source_dir);
    let t = if is_project {
        ItemType::Project
    } else {
        ItemType::Unknown
    };
    (t, confidence)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn appdata_caches_and_stores() {
        assert_eq!(
            path_noise(&p(r"C:\Users\rnz\AppData\Local\Temp")),
            Some(ItemType::Cache)
        );
        assert_eq!(
            path_noise(&p(r"C:\Users\rnz\AppData\Local\pnpm\store")),
            Some(ItemType::DependencyStore)
        );
        assert_eq!(
            path_noise(&p(r"C:\Users\rnz\AppData\Local\_npx")),
            Some(ItemType::Cache)
        );
        assert_eq!(
            path_noise(&p(r"C:\Users\rnz\AppData\Roaming\Code")),
            Some(ItemType::ApplicationData)
        );
        // AppData containers themselves are traversed, not leaf-classified.
        assert_eq!(path_noise(&p(r"C:\Users\rnz\AppData\Local")), None);
        assert_eq!(path_noise(&p(r"C:\Users\rnz\AppData")), None);
    }

    #[test]
    fn dependency_stores_anywhere() {
        assert_eq!(
            path_noise(&p(r"C:\Users\rnz\go\pkg\mod\x")),
            Some(ItemType::DependencyStore)
        );
        assert_eq!(
            path_noise(&p(r"C:\Users\rnz\.cargo\registry\src")),
            Some(ItemType::DependencyStore)
        );
    }

    #[test]
    fn real_workspaces_are_not_noise() {
        assert_eq!(path_noise(&p(r"C:\Users\rnz\Downloads\client-a")), None);
        assert_eq!(path_noise(&p(r"E:\Projects\client-a\frontend")), None);
        assert_eq!(path_noise(&p(r"C:\Users\rnz\Documents\proj")), None);
    }

    #[test]
    fn files_by_extension() {
        assert_eq!(file_type(&p("a/random.zip")), ItemType::Archive);
        assert_eq!(file_type(&p("a/installer.exe")), ItemType::File);
        assert_eq!(file_type(&p("a/notes")), ItemType::File);
    }

    #[test]
    fn no_git_project_still_a_project() {
        let s = Signals {
            manifest: true,
            source_dir: true,
            ..Default::default()
        };
        let (t, _c) = classify_project(s, false);
        assert_eq!(t, ItemType::Project);
    }

    #[test]
    fn empty_dir_is_unknown_not_project() {
        let (t, _c) = classify_project(Signals::default(), false);
        assert_eq!(t, ItemType::Unknown);
    }
}
