//! Project detection: decide whether a directory is a project root and
//! infer its technology stack from marker files.

use std::fs;
use std::path::Path;

/// Marker file names that, if present directly inside a directory, mark it
/// as a project root.
const MARKER_FILES: &[&str] = &[
    "package.json",
    "Cargo.toml",
    "composer.json",
    "go.mod",
    "pyproject.toml",
    "requirements.txt",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "Dockerfile",
    "docker-compose.yml",
    "compose.yml",
];

/// Marker file extensions (e.g. *.sln, *.csproj).
const MARKER_EXTS: &[&str] = &["sln", "csproj"];

/// Returns true if `dir` is a project root (contains `.git` or any marker).
pub fn is_project_root(dir: &Path) -> bool {
    if dir.join(".git").exists() || is_wordpress(dir) {
        return true;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return false,
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if MARKER_FILES.iter().any(|m| m.eq_ignore_ascii_case(&name)) {
            return true;
        }
        if let Some(ext) = Path::new(name.as_ref()).extension() {
            let ext = ext.to_string_lossy().to_lowercase();
            if MARKER_EXTS.contains(&ext.as_str()) {
                return true;
            }
        }
    }
    false
}

/// Detect the technology stack of a project directory.
pub fn detect_stack(dir: &Path) -> Vec<String> {
    let mut stack: Vec<String> = Vec::new();

    if dir.join("package.json").exists() {
        stack.push("Node.js".to_string());
        if let Some(fw) = node_framework(&dir.join("package.json")) {
            stack.push(fw);
        }
    }
    if dir.join("Cargo.toml").exists() {
        stack.push("Rust".to_string());
    }
    if dir.join("composer.json").exists() {
        stack.push("PHP".to_string());
    }
    if dir.join("go.mod").exists() {
        stack.push("Go".to_string());
    }
    if dir.join("pyproject.toml").exists() || dir.join("requirements.txt").exists() {
        stack.push("Python".to_string());
    }
    if has_ext_in_dir(dir, "sln") || has_ext_in_dir(dir, "csproj") {
        stack.push(".NET".to_string());
    }

    if is_wordpress(dir) && !stack.iter().any(|x| x.as_str() == "WordPress") {
        stack.insert(0, "WordPress".to_string());
    }

    if stack.is_empty() && dir.join(".git").exists() {
        stack.push("Git".to_string());
    }
    stack
}

/// Read package.json and try to recognise a popular framework from its deps.
fn node_framework(pkg_json: &Path) -> Option<String> {
    let text = fs::read_to_string(pkg_json).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let mut deps = serde_json::Map::new();
    for key in ["dependencies", "devDependencies"] {
        if let Some(obj) = json.get(key).and_then(|v| v.as_object()) {
            for (k, v) in obj {
                deps.insert(k.clone(), v.clone());
            }
        }
    }
    // Order matters: check the most specific frameworks first.
    let checks: &[(&str, &str)] = &[
        ("next", "Next.js"),
        ("nuxt", "Nuxt"),
        ("@angular/core", "Angular"),
        ("svelte", "Svelte"),
        ("vue", "Vue"),
        ("@remix-run/react", "Remix"),
        ("astro", "Astro"),
        ("react", "React"),
        ("vite", "Vite"),
        ("express", "Express"),
    ];
    for (dep, label) in checks {
        if deps.contains_key(*dep) {
            return Some(label.to_string());
        }
    }
    None
}

fn has_ext_in_dir(dir: &Path, ext: &str) -> bool {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if let Some(e) = Path::new(&name).extension() {
                if e.to_string_lossy().eq_ignore_ascii_case(ext) {
                    return true;
                }
            }
        }
    }
    false
}

/// Whether the directory contains a README (any common variant).
pub fn has_readme(dir: &Path) -> bool {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let lower = name.to_string_lossy().to_lowercase();
            if lower.starts_with("readme") {
                return true;
            }
        }
    }
    false
}

/// Weak signal: a config file that projects commonly carry (tsconfig,
/// vite/next/astro/nuxt config, .gitignore, .env).
pub fn has_config(dir: &Path) -> bool {
    let named = [
        "tsconfig.json",
        ".gitignore",
        ".env",
        ".env.local",
        "Makefile",
        "vercel.json",
    ];
    if named.iter().any(|n| dir.join(n).exists()) {
        return true;
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let lower = name.to_string_lossy().to_lowercase();
            // vite.config.*, next.config.*, astro.config.*, nuxt.config.*, svelte.config.*
            if lower.ends_with(".config.js")
                || lower.ends_with(".config.ts")
                || lower.ends_with(".config.mjs")
                || lower.ends_with(".config.cjs")
            {
                return true;
            }
        }
    }
    false
}

/// Weak signal: a conventional source directory.
pub fn has_source_dir(dir: &Path) -> bool {
    const SRC_DIRS: &[&str] = &[
        "src", "app", "server", "client", "frontend", "backend", "public", "lib", "tests", "test",
    ];
    SRC_DIRS.iter().any(|d| dir.join(d).is_dir())
}

/// Whether the directory contains any project manifest (marker file), excluding .git.
pub fn has_manifest(dir: &Path) -> bool {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return false,
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if MARKER_FILES.iter().any(|m| m.eq_ignore_ascii_case(&name)) {
            return true;
        }
        if let Some(ext) = Path::new(name.as_ref()).extension() {
            let ext = ext.to_string_lossy().to_lowercase();
            if MARKER_EXTS.contains(&ext.as_str()) {
                return true;
            }
        }
    }
    false
}

/// Directory names that hold vendored dependencies, not user projects. The
/// scanner never descends into these when searching for project roots, so a
/// `composer.json`/`package.json` deep inside a dependency tree is not mistaken
/// for a real project.
pub const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "vendor",
    "vendor_prefixed",
    "bower_components",
    ".pnpm",
    ".yarn",
    "wp-content",
    "wp-includes",
    "wp-admin",
];

/// Whether traversal should skip (never enter) this directory.
pub fn is_skip_dir(name: &str) -> bool {
    SKIP_DIRS.contains(&name)
}

/// Whether the directory is the root of a WordPress install.
///
/// Requires the real WordPress structure: a `wp-includes` directory **plus** a
/// core loader file. This deliberately does NOT trigger on a stray
/// `wp-config.php` sitting loose in a folder (common in Downloads), which would
/// otherwise mis-classify the whole folder as one giant WordPress project.
pub fn is_wordpress(dir: &Path) -> bool {
    dir.join("wp-includes").is_dir()
        && (dir.join("wp-load.php").exists()
            || dir.join("wp-login.php").exists()
            || dir.join("wp-settings.php").exists())
}
