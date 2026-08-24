//! Read-only Git audit. Runs only inspection commands (`status`, `remote`,
//! `log`) via `git -C <dir>` — never fetch/reset/clean/checkout. No network:
//! remote *reachability* is intentionally left to manual verification so the
//! audit stays strictly local.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use crate::model::GitInfo;

/// Run a read-only git command inside `dir`, returning stdout on success.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        None
    }
}

/// Strip embedded credentials from a remote URL (`scheme://user:pass@host` →
/// `scheme://host`) so tokens never reach the report.
fn redact(url: &str) -> String {
    if let Some(scheme_end) = url.find("://") {
        let after = &url[scheme_end + 3..];
        if let Some(at) = after.find('@') {
            return format!("{}://{}", &url[..scheme_end], &after[at + 1..]);
        }
    }
    url.to_string()
}

/// Audit a directory's Git state (read-only). Returns a default (is_repo=false)
/// GitInfo when `dir` is not a repository.
pub fn audit(dir: &Path) -> GitInfo {
    let mut g = GitInfo::default();
    if !dir.join(".git").exists() {
        return g;
    }
    // One call gives branch, upstream, ahead/behind and the file-status list.
    let porcelain = match git(dir, &["status", "--porcelain=v2", "--branch"]) {
        Some(s) => s,
        None => return g, // .git present but not a valid work tree
    };
    g.is_repo = true;

    for line in porcelain.lines() {
        if let Some(rest) = line.strip_prefix("# branch.head ") {
            g.branch = rest.trim().to_string();
            g.detached = g.branch == "(detached)";
        } else if let Some(rest) = line.strip_prefix("# branch.oid ") {
            let sha = rest.trim();
            if !sha.starts_with('(') {
                g.head = sha.chars().take(8).collect();
            }
        } else if let Some(rest) = line.strip_prefix("# branch.upstream ") {
            g.has_upstream = !rest.trim().is_empty();
        } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
            for tok in rest.split_whitespace() {
                if let Some(a) = tok.strip_prefix('+') {
                    g.ahead = a.parse().unwrap_or(0);
                } else if let Some(b) = tok.strip_prefix('-') {
                    g.behind = b.parse().unwrap_or(0);
                }
            }
        } else if line.starts_with("1 ") || line.starts_with("2 ") || line.starts_with("u ") {
            g.modified += 1;
        } else if line.starts_with("? ") {
            g.untracked += 1;
        }
    }
    g.dirty = g.modified > 0;

    if let Some(out) = git(dir, &["remote", "-v"]) {
        let mut seen = BTreeSet::new();
        for line in out.lines() {
            // "<name>\t<url> (fetch|push)"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                seen.insert(redact(parts[1]));
            }
        }
        g.remotes = seen.into_iter().collect();
    }
    g.has_remote = !g.remotes.is_empty();

    if let Some(out) = git(dir, &["log", "-1", "--format=%ct"]) {
        g.last_commit = out.trim().parse().unwrap_or(0);
    }
    g
}

#[cfg(test)]
mod tests {
    use super::redact;
    #[test]
    fn redacts_credentials() {
        assert_eq!(
            redact("https://user:tok@github.com/a/b.git"),
            "https://github.com/a/b.git"
        );
        assert_eq!(redact("git@github.com:a/b.git"), "git@github.com:a/b.git");
        assert_eq!(
            redact("https://github.com/a/b.git"),
            "https://github.com/a/b.git"
        );
    }
}
