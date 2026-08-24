//! Read-only secrets/environment audit. Detects top-level `.env*` files, counts
//! how many variables they declare, and asks Git whether each is tracked.
//! The file *content/values* are NEVER captured — only counts and flags.

use std::path::Path;
use std::process::Command;

use crate::model::EnvFile;

/// Example/template env files that don't hold real secrets.
const IGNORE: &[&str] = &[
    ".env.example",
    ".env.sample",
    ".env.template",
    ".env.dist",
    ".env.defaults",
];

/// Scan the top level of `dir` for `.env*` files.
pub fn scan(dir: &Path) -> Vec<EnvFile> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let lower = name.to_lowercase();
        if !lower.starts_with(".env") || IGNORE.contains(&lower.as_str()) {
            continue;
        }
        out.push(EnvFile {
            path: path.to_string_lossy().to_string(),
            var_count: count_vars(&path),
            tracked_by_git: tracked_by_git(dir, &name),
            name,
        });
    }
    out
}

/// Count declared variables (non-empty, non-comment lines containing `=`).
/// Reads the file but keeps nothing — values are never returned or logged.
fn count_vars(p: &Path) -> u32 {
    let text = match std::fs::read_to_string(p) {
        Ok(t) => t,
        Err(_) => return 0,
    };
    text.lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('#') && t.contains('=')
        })
        .count() as u32
}

/// Whether Git tracks `name` inside `dir` (read-only). A tracked `.env` is a
/// committed-secret risk.
fn tracked_by_git(dir: &Path, name: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["ls-files", "--error-unmatch", "--", name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
