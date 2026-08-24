//! Read-only installed-software inventory. Runs each tool's version command and
//! parses a version string. On Windows the tools are usually `.cmd` shims, so
//! commands are launched through `cmd /C` (mirrors how VS Code is opened).

use std::process::Command;

use crate::model::SoftwareItem;

/// (label, program, args). Order roughly follows the spec's tool list.
const TOOLS: &[(&str, &str, &[&str])] = &[
    ("Git", "git", &["--version"]),
    ("GitHub CLI", "gh", &["--version"]),
    ("Node.js", "node", &["--version"]),
    ("npm", "npm", &["--version"]),
    ("pnpm", "pnpm", &["--version"]),
    ("Yarn", "yarn", &["--version"]),
    ("Bun", "bun", &["--version"]),
    ("Python", "python", &["--version"]),
    ("pip", "pip", &["--version"]),
    ("uv", "uv", &["--version"]),
    ("Poetry", "poetry", &["--version"]),
    ("Rust (rustc)", "rustc", &["--version"]),
    ("Cargo", "cargo", &["--version"]),
    ("Go", "go", &["version"]),
    ("Java", "java", &["-version"]),
    (".NET", "dotnet", &["--version"]),
    ("PHP", "php", &["--version"]),
    ("Ruby", "ruby", &["--version"]),
    ("Docker", "docker", &["--version"]),
    ("Docker Compose", "docker", &["compose", "version"]),
    ("WSL", "wsl", &["--version"]),
    ("VS Code", "code", &["--version"]),
    ("Vercel CLI", "vercel", &["--version"]),
    ("kubectl", "kubectl", &["version", "--client"]),
    ("Terraform", "terraform", &["--version"]),
];

/// Probe every tool and return the inventory (found + version, or not found).
pub fn inventory() -> Vec<SoftwareItem> {
    TOOLS
        .iter()
        .map(|(label, program, args)| {
            let (found, version) = probe(program, args);
            SoftwareItem {
                name: label.to_string(),
                version,
                found,
            }
        })
        .collect()
}

/// Run `program args`, returning (found, version). Combines stdout+stderr since
/// some tools (e.g. `java -version`) print the version to stderr.
fn probe(program: &str, args: &[&str]) -> (bool, String) {
    let output = if cfg!(target_os = "windows") {
        Command::new("cmd").arg("/C").arg(program).args(args).output()
    } else {
        Command::new(program).args(args).output()
    };
    match output {
        Ok(o) if o.status.success() => {
            let mut text = String::from_utf8_lossy(&o.stdout).to_string();
            text.push_str(&String::from_utf8_lossy(&o.stderr));
            let v = parse_version(&text);
            (true, if v.is_empty() { "installed".into() } else { v })
        }
        _ => (false, String::new()),
    }
}

/// Extract the first version-looking token (has a digit and a dot). Strips
/// UTF-16/control bytes that tools like `wsl.exe` emit.
fn parse_version(s: &str) -> String {
    for raw in s.split(|c: char| c.is_whitespace() || c == ',' || c == '(' || c == ')') {
        let tok: String = raw.chars().filter(|c| c.is_ascii_graphic()).collect();
        let t = tok.trim_start_matches(|c: char| !c.is_ascii_digit());
        if t.contains('.') && t.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return t
                .trim_end_matches(|c: char| !c.is_ascii_alphanumeric())
                .to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::parse_version;
    #[test]
    fn parses_common_shapes() {
        assert_eq!(parse_version("git version 2.43.0"), "2.43.0");
        assert_eq!(parse_version("v20.11.1"), "20.11.1");
        assert_eq!(parse_version("go version go1.22.0 windows/amd64"), "1.22.0");
        assert_eq!(parse_version("Python 3.12.1"), "3.12.1");
        assert_eq!(parse_version("no version here"), "");
    }
}
