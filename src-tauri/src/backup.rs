//! Dev-aware backup: copy a project's source (including `.git` history) to a
//! destination, skipping regenerable dependency/build directories.

use std::fs;
use std::path::Path;

use walkdir::WalkDir;

/// Directories excluded from backups (regenerable deps/build output). `.git`
/// is intentionally kept — it's the project's history.
const EXCLUDE: &[&str] = &[
    "node_modules",
    ".next",
    "dist",
    "build",
    "coverage",
    "target",
    "bin",
    "obj",
    "__pycache__",
    ".venv",
    "vendor",
    "vendor_prefixed",
    "bower_components",
    ".pnpm",
    ".yarn",
];

fn excluded(name: &str) -> bool {
    EXCLUDE.contains(&name)
}

/// Copy `src` into `dest_dir`, skipping excluded directories. Returns
/// (files_copied, bytes_copied). Best-effort: unreadable files are skipped.
pub fn copy_clean(src: &Path, dest_dir: &Path) -> std::io::Result<(u64, u64)> {
    let mut files = 0u64;
    let mut bytes = 0u64;
    fs::create_dir_all(dest_dir)?;

    let walker = WalkDir::new(src).follow_links(false).into_iter().filter_entry(|e| {
        let name = e.file_name().to_string_lossy();
        !(e.file_type().is_dir() && excluded(&name))
    });

    for entry in walker.flatten() {
        let rel = match entry.path().strip_prefix(src) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let target = dest_dir.join(rel);
        if entry.file_type().is_dir() {
            let _ = fs::create_dir_all(&target);
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(n) = fs::copy(entry.path(), &target) {
                files += 1;
                bytes += n;
            }
        }
    }
    Ok((files, bytes))
}
