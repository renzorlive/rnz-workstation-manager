//! Reinstall readiness + recovery pack export.
//!
//! Detects developer configuration artifacts (keys, AI tools, editors, shells,
//! cloud CLIs, package managers) and can bundle them into a single recovery zip
//! so a workstation can be rebuilt quickly after a disk failure / reinstall.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;
use zip::write::SimpleFileOptions;

use crate::model::{Readiness, ReadinessItem};

/// One recovery artifact and where to look for it.
struct Spec {
    key: &'static str,
    label: &'static str,
    category: &'static str,
    candidates: Vec<PathBuf>,
    secret: bool,
    essential: bool,
}

fn s(
    key: &'static str,
    label: &'static str,
    category: &'static str,
    candidates: Vec<PathBuf>,
    secret: bool,
    essential: bool,
) -> Spec {
    Spec { key, label, category, candidates, secret, essential }
}

/// Build the list of artifacts to check, resolved against the current user dirs.
fn specs() -> Vec<Spec> {
    let home = dirs::home_dir().unwrap_or_default();
    let cfg = home.join(".config"); // some CLIs use ~/.config even on Windows
    let roaming = dirs::config_dir().unwrap_or_default(); // %APPDATA%
    let local = dirs::data_local_dir().unwrap_or_default(); // %LOCALAPPDATA%
    let docs = dirs::document_dir().unwrap_or_default();

    let editor = |vendor: &str| roaming.join(vendor).join("User").join("settings.json");

    vec![
        // ---- Identity & keys ----
        s("gitconfig", "Git config", "Identity & keys", vec![home.join(".gitconfig")], false, true),
        s("git_dir", "Git config dir", "Identity & keys", vec![cfg.join("git")], false, false),
        s("ssh", "SSH keys", "Identity & keys", vec![home.join(".ssh")], true, true),
        s("gpg", "GPG keys", "Identity & keys", vec![home.join(".gnupg")], true, false),
        s("npmrc", "npm config (.npmrc)", "Identity & keys", vec![home.join(".npmrc")], true, false),
        s("cargo_creds", "Cargo credentials", "Identity & keys", vec![home.join(".cargo").join("credentials.toml"), home.join(".cargo").join("config.toml")], true, false),

        // ---- AI coding tools ----
        s("claude", "Claude", "AI tools", vec![home.join(".claude"), home.join(".claude.json")], false, false),
        s("codex", "Codex (OpenAI)", "AI tools", vec![home.join(".codex")], false, false),
        s("gemini", "Gemini CLI", "AI tools", vec![home.join(".gemini"), cfg.join("gemini")], false, false),
        s("opencode", "opencode", "AI tools", vec![home.join(".opencode"), cfg.join("opencode"), local.join("opencode")], false, false),
        s("aider", "Aider", "AI tools", vec![home.join(".aider.conf.yml"), home.join(".aider")], false, false),
        s("continue", "Continue", "AI tools", vec![home.join(".continue")], false, false),
        s("cursor_home", "Cursor (home)", "AI tools", vec![home.join(".cursor")], false, false),
        s("codeium", "Codeium / Windsurf", "AI tools", vec![home.join(".codeium"), home.join(".windsurf")], false, false),
        s("manus", "Manus", "AI tools", vec![home.join(".manus"), cfg.join("manus")], false, false),

        // ---- Editors ----
        s("vscode", "VS Code settings", "Editors", vec![editor("Code")], false, true),
        s("vscode_keys", "VS Code keybindings", "Editors", vec![roaming.join("Code").join("User").join("keybindings.json")], false, false),
        s("vscode_snippets", "VS Code snippets", "Editors", vec![roaming.join("Code").join("User").join("snippets")], false, false),
        s("vscode_insiders", "VS Code Insiders", "Editors", vec![editor("Code - Insiders")], false, false),
        s("cursor", "Cursor settings", "Editors", vec![editor("Cursor")], false, false),
        s("windsurf", "Windsurf settings", "Editors", vec![editor("Windsurf")], false, false),
        s("trae", "Trae settings", "Editors", vec![editor("Trae")], false, false),
        s("zed", "Zed", "Editors", vec![roaming.join("Zed").join("settings.json"), local.join("Zed").join("settings.json")], false, false),
        s("neovim", "Neovim", "Editors", vec![local.join("nvim"), cfg.join("nvim")], false, false),
        s("sublime", "Sublime Text", "Editors", vec![roaming.join("Sublime Text").join("Packages").join("User")], false, false),
        s("jetbrains", "JetBrains", "Editors", vec![roaming.join("JetBrains")], false, false),

        // ---- Shell & terminal ----
        s("powershell", "PowerShell profile", "Shell & terminal", vec![docs.join("PowerShell").join("Microsoft.PowerShell_profile.ps1"), docs.join("WindowsPowerShell").join("Microsoft.PowerShell_profile.ps1")], false, true),
        s("windows_terminal", "Windows Terminal", "Shell & terminal", windows_terminal_settings(&local), false, true),
        s("bashrc", "Bash (.bashrc)", "Shell & terminal", vec![home.join(".bashrc"), home.join(".bash_profile")], false, false),
        s("zshrc", "Zsh (.zshrc)", "Shell & terminal", vec![home.join(".zshrc")], false, false),
        s("starship", "Starship prompt", "Shell & terminal", vec![cfg.join("starship.toml")], false, false),
        s("wezterm", "WezTerm", "Shell & terminal", vec![home.join(".wezterm.lua"), cfg.join("wezterm")], false, false),

        // ---- Cloud & CLIs ----
        s("gh", "GitHub CLI", "Cloud & CLIs", vec![roaming.join("GitHub CLI"), cfg.join("gh")], true, false),
        s("docker", "Docker config", "Cloud & CLIs", vec![home.join(".docker").join("config.json")], true, false),
        s("kube", "kubectl config", "Cloud & CLIs", vec![home.join(".kube").join("config")], true, false),
        s("aws", "AWS credentials", "Cloud & CLIs", vec![home.join(".aws")], true, false),
        s("gcloud", "gcloud config", "Cloud & CLIs", vec![roaming.join("gcloud"), cfg.join("gcloud")], true, false),
        s("azure", "Azure CLI", "Cloud & CLIs", vec![home.join(".azure")], true, false),

        // ---- Package managers ----
        s("yarn", "Yarn config", "Package managers", vec![home.join(".yarnrc.yml"), home.join(".yarnrc")], false, false),
        s("pip", "pip config", "Package managers", vec![roaming.join("pip").join("pip.ini"), cfg.join("pip").join("pip.conf")], false, false),
        s("maven", "Maven settings", "Package managers", vec![home.join(".m2").join("settings.xml")], false, false),
        s("nuget", "NuGet config", "Package managers", vec![roaming.join("NuGet").join("NuGet.Config")], false, false),
    ]
}

fn windows_terminal_settings(local: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let packages = local.join("Packages");
    if let Ok(rd) = fs::read_dir(&packages) {
        for entry in rd.flatten() {
            if entry.file_name().to_string_lossy().starts_with("Microsoft.WindowsTerminal") {
                out.push(entry.path().join("LocalState").join("settings.json"));
            }
        }
    }
    out.push(local.join("Microsoft").join("Windows Terminal").join("settings.json"));
    out
}

fn path_size(p: &Path) -> u64 {
    if p.is_file() {
        return fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    }
    WalkDir::new(p)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// Compute the reinstall-readiness checklist and score (score over essentials).
pub fn readiness() -> Readiness {
    let specs = specs();
    let mut items = Vec::new();
    let mut essential_total = 0i32;
    let mut essential_present = 0i32;

    for sp in &specs {
        let found = sp.candidates.iter().find(|c| c.exists());
        let present = found.is_some();
        if sp.essential {
            essential_total += 1;
            if present {
                essential_present += 1;
            }
        }
        let path = found
            .or_else(|| sp.candidates.first())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let size_bytes = found.map(|p| path_size(p)).unwrap_or(0);
        items.push(ReadinessItem {
            key: sp.key.to_string(),
            label: sp.label.to_string(),
            category: sp.category.to_string(),
            path,
            present,
            size_bytes,
            secret: sp.secret,
            essential: sp.essential,
        });
    }

    let score = if essential_total > 0 {
        (essential_present as f64 / essential_total as f64 * 100.0).round() as i32
    } else {
        0
    };
    Readiness { score, items }
}

/// Build a recovery zip at `dest` containing every present artifact plus the
/// supplied project-inventory JSON.
pub fn build_pack(dest: &Path, extras: &[(&str, String)]) -> std::io::Result<()> {
    let file = File::create(dest)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    for sp in specs() {
        if let Some(found) = sp.candidates.iter().find(|c| c.exists()) {
            add_to_zip(&mut zip, sp.key, found, opts)?;
        }
    }

    for (name, content) in extras {
        zip.start_file((*name).to_string(), opts)?;
        zip.write_all(content.as_bytes())?;
    }
    zip.finish()?;
    Ok(())
}

fn add_to_zip(
    zip: &mut zip::ZipWriter<File>,
    key: &str,
    src: &Path,
    opts: SimpleFileOptions,
) -> std::io::Result<()> {
    if src.is_dir() {
        for entry in WalkDir::new(src).into_iter().flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let rel = entry.path().strip_prefix(src).unwrap_or(entry.path());
            let name = format!("{key}/{}", rel.to_string_lossy().replace('\\', "/"));
            zip.start_file(name, opts)?;
            let bytes = fs::read(entry.path())?;
            zip.write_all(&bytes)?;
        }
    } else {
        let fname = src
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| key.to_string());
        zip.start_file(format!("{key}/{fname}"), opts)?;
        let bytes = fs::read(src)?;
        zip.write_all(&bytes)?;
    }
    Ok(())
}
