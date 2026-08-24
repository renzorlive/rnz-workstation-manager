//! Workstation Asset Discovery (read-only). Separate from Project Discovery:
//! it enumerates the developer assets that live OUTSIDE projects — dotfiles /
//! dotdirs under the home directory and the app folders under AppData — so the
//! workstation can be rebuilt after a Windows reinstall.
//!
//! Assets are DISCOVERED from the real filesystem, not guessed from a fixed
//! list. Contents are never read; only names, sizes and a secret flag are
//! recorded. This complements `recovery.rs` (a curated checklist of known-good
//! config paths) with "what actually exists on this machine".

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use walkdir::WalkDir;

use crate::model::{Asset, AssetCategory, AssetInventory};

/// Stop sizing an asset after this many files (browser caches etc. can hold
/// millions). `size_bytes` then becomes a lower bound and `truncated` is set.
const FILE_CAP: u64 = 50_000;

/// Discover every home dotfile/dotdir and top-level AppData app folder.
pub fn discover(now: i64) -> AssetInventory {
    let mut assets: Vec<Asset> = Vec::new();

    if let Some(home) = dirs::home_dir() {
        scan_dir(&home, "home", true, &mut assets);
    }
    // %APPDATA% == Roaming; %LOCALAPPDATA% == Local; LocalLow is a sibling.
    if let Some(roaming) = dirs::config_dir() {
        scan_dir(&roaming, "roaming", false, &mut assets);
        if let Some(appdata) = roaming.parent() {
            scan_dir(&appdata.join("LocalLow"), "locallow", false, &mut assets);
        }
    }
    if let Some(local) = dirs::data_local_dir() {
        scan_dir(&local, "local", false, &mut assets);
    }

    assets.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));

    let total_size_bytes = assets.iter().map(|a| a.size_bytes).sum();
    let secret_count = assets.iter().filter(|a| a.secret).count();

    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for a in &assets {
        *counts.entry(a.category.as_str()).or_insert(0) += 1;
    }
    let mut by_category: Vec<(String, usize)> =
        counts.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    by_category.sort_by(|a, b| b.1.cmp(&a.1));

    AssetInventory {
        generated_at: now,
        assets,
        total_size_bytes,
        secret_count,
        by_category,
    }
}

/// Enumerate the direct children of `base`. When `dotfiles_only`, only entries
/// starting with `.` are taken (home dir); otherwise every entry (AppData).
fn scan_dir(base: &Path, location: &str, dotfiles_only: bool, out: &mut Vec<Asset>) {
    let entries = match fs::read_dir(base) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if dotfiles_only && !name.starts_with('.') {
            continue;
        }
        if !dotfiles_only && is_appdata_noise(&name) {
            continue;
        }
        let path = entry.path();
        let (category, secret) = classify(&name, location);
        // Caches are regenerable and often enormous (many small files) — don't
        // pay to deep-walk them; a shallow size is enough to show they exist.
        let (size_bytes, file_count, truncated) = if category == AssetCategory::Cache {
            shallow_size(&path)
        } else {
            sized(&path)
        };
        out.push(Asset {
            name,
            path: path.to_string_lossy().to_string(),
            category,
            location: location.to_string(),
            size_bytes,
            file_count,
            secret,
            truncated,
        });
    }
}

/// AppData entries that are pure OS noise, not worth inventorying.
fn is_appdata_noise(name: &str) -> bool {
    matches!(name, "Temp" | "Microsoft" | "Packages" | "ConnectedDevicesPlatform")
}

/// Direct-children-only size (for caches we don't want to deep-walk). Marked
/// truncated since nested content is not counted.
fn shallow_size(path: &Path) -> (u64, u64, bool) {
    if path.is_file() {
        let b = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        return (b, 1, false);
    }
    let mut bytes = 0u64;
    let mut files = 0u64;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(m) = entry.metadata() {
                if m.is_file() {
                    bytes += m.len();
                    files += 1;
                }
            }
        }
    }
    (bytes, files, true)
}

/// Recursively size a path, capped at `FILE_CAP` files.
fn sized(path: &Path) -> (u64, u64, bool) {
    if path.is_file() {
        let b = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        return (b, 1, false);
    }
    let mut bytes = 0u64;
    let mut files = 0u64;
    for entry in WalkDir::new(path).follow_links(false).into_iter().flatten() {
        if entry.file_type().is_file() {
            if let Ok(m) = entry.metadata() {
                bytes += m.len();
            }
            files += 1;
            if files >= FILE_CAP {
                return (bytes, files, true);
            }
        }
    }
    (bytes, files, false)
}

/// Classify an asset by name + location into (category, is_secret).
fn classify(name: &str, location: &str) -> (AssetCategory, bool) {
    use AssetCategory::*;
    let n = name.to_lowercase();

    if location == "home" {
        // Home dotfiles / dotdirs.
        return match n.as_str() {
            ".ssh" | ".gnupg" => (Credentials, true),
            ".aws" | ".azure" | ".kube" | ".gcloud" | ".oci" => (CloudCli, true),
            ".docker" => (Docker, true),
            ".git-credentials" | ".netrc" => (Credentials, true),
            ".npmrc" | ".yarnrc" | ".yarnrc.yml" | ".pnpmrc" => (PackageManager, true),
            ".gitconfig" | ".gitignore_global" | ".wslconfig" | ".editorconfig" => (Config, false),
            ".bun" | ".deno" | ".nvm" | ".volta" | ".cargo" | ".rustup" | ".m2" | ".gradle"
            | ".nuget" | ".pyenv" | ".rbenv" | ".sdkman" => (PackageManager, false),
            ".ollama" | ".lmstudio" => (LocalAi, false),
            ".claude" | ".claude.json" | ".codex" | ".gemini" | ".continue" | ".codeium"
            | ".cursor" | ".windsurf" | ".aider.conf.yml" | ".manus" => (AiAgent, false),
            ".vscode" | ".vscode-oss" | ".vscode-server" => (Editor, false),
            ".config" | ".local" => (Config, false),
            ".cache" => (Cache, false),
            ".bashrc" | ".bash_profile" | ".zshrc" | ".profile" | ".wezterm.lua"
            | ".inputrc" => (Shell, false),
            _ if n.starts_with(".aider") => (AiAgent, false),
            _ => (Config, false), // dotfiles default to config
        };
    }

    // AppData (roaming / local / locallow) — folders named after apps.
    match n.as_str() {
        "code" | "code - insiders" | "cursor" | "windsurf" | "trae" | "jetbrains"
        | "sublime text" | "sublime text 3" | "zed" | "neovim" | "nvim" => (Editor, false),
        "google" | "chromium" | "mozilla" | "bravesoftware" | "vivaldi" | "opera software"
        | "opera" | "microsoft edge" | "yandex" => (Browser, false),
        "docker" | "docker desktop" => (Docker, true),
        "npm" | "pnpm" | "yarn" | "bun" => (PackageManager, false),
        "npm-cache" | "pip" => (Cache, false),
        "postgresql" | "mysql" | "mariadb" | "mongodb" | "redis" | "pgadmin" => (Database, false),
        "ollama" | "lm studio" | "nomic.ai" | "jan" => (LocalAi, false),
        "github cli" | "gcloud" | "google\\cloud sdk" => (CloudCli, true),
        "gnupg" | "gnupg2" => (Credentials, true),
        _ if n.contains("cache") => (Cache, false),
        _ => (Application, false),
    }
}

#[cfg(test)]
mod tests {
    use super::classify;
    use crate::model::AssetCategory::*;

    #[test]
    fn home_credentials_are_secret() {
        assert_eq!(classify(".ssh", "home"), (Credentials, true));
        assert_eq!(classify(".aws", "home"), (CloudCli, true));
        assert_eq!(classify(".npmrc", "home"), (PackageManager, true));
    }

    #[test]
    fn ai_and_local_ai() {
        assert_eq!(classify(".claude", "home").0, AiAgent);
        assert_eq!(classify(".ollama", "home").0, LocalAi);
        assert_eq!(classify("Ollama", "local").0, LocalAi);
    }

    #[test]
    fn appdata_apps_and_browsers() {
        assert_eq!(classify("Code", "roaming").0, Editor);
        assert_eq!(classify("Google", "local").0, Browser);
        assert_eq!(classify("SomeRandomApp", "roaming").0, Application);
    }

    #[test]
    fn unknown_dotfile_defaults_to_config() {
        assert_eq!(classify(".somerc", "home"), (Config, false));
    }
}
