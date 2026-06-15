//! Junk classification: which directories are recoverable build/dependency
//! artifacts, and which files are archives.

/// Directory names treated as recoverable junk.
pub const JUNK_DIRS: &[&str] = &[
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
];

/// Archive file extensions counted in the "archives" bucket.
pub const ARCHIVE_EXTS: &[&str] = &["zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "xz"];

/// Whether a directory name is any kind of junk dir (used to prune walks).
pub fn is_junk_dir(name: &str) -> bool {
    JUNK_DIRS.contains(&name)
}

/// Whether a file extension is an archive.
pub fn is_archive_ext(ext: &str) -> bool {
    let lower = ext.to_lowercase();
    ARCHIVE_EXTS.contains(&lower.as_str())
}

/// Canonical junk category for a directory name (one bucket per dir type).
pub fn category_for_dir(name: &str) -> Option<&'static str> {
    match name {
        "node_modules" => Some("node_modules"),
        ".next" => Some(".next"),
        "dist" => Some("dist"),
        "build" => Some("build"),
        "coverage" => Some("coverage"),
        "target" => Some("target"),
        "bin" => Some("bin"),
        "obj" => Some("obj"),
        "__pycache__" => Some("__pycache__"),
        ".venv" => Some(".venv"),
        _ => None,
    }
}
