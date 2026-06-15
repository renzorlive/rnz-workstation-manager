//! Docker awareness: capture a manifest of what exists (so it can be recreated)
//! and back up named volumes (the actual project data) to tarballs.

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::model::{DockerBackupResult, DockerContainer, DockerStatus};

/// Run a docker command, returning stdout on success.
fn run(args: &[&str]) -> Option<String> {
    let out = Command::new("docker").args(args).output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        None
    }
}

fn lines(s: Option<String>) -> Vec<String> {
    s.unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Best-effort snapshot of the local Docker state.
pub fn status() -> DockerStatus {
    let available = run(&["--version"]).is_some();
    let running = run(&["info", "--format", "{{.ServerVersion}}"])
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    let containers = lines(run(&[
        "ps",
        "-a",
        "--format",
        "{{.Names}}\t{{.Image}}\t{{.State}}",
    ]))
    .into_iter()
    .map(|l| {
        let mut parts = l.splitn(3, '\t');
        DockerContainer {
            name: parts.next().unwrap_or("").to_string(),
            image: parts.next().unwrap_or("").to_string(),
            state: parts.next().unwrap_or("").to_string(),
        }
    })
    .collect();

    DockerStatus {
        available,
        running,
        containers,
        images: lines(run(&["images", "--format", "{{.Repository}}:{{.Tag}}"])),
        volumes: lines(run(&["volume", "ls", "--format", "{{.Name}}"])),
        networks: lines(run(&["network", "ls", "--format", "{{.Name}}"])),
    }
}

/// Export every named Docker volume into `dest/docker-volumes/<name>.tar.gz`
/// using a throwaway alpine container. Requires Docker to be running.
pub fn export_volumes(dest: &Path) -> DockerBackupResult {
    let out_dir = dest.join("docker-volumes");
    let _ = fs::create_dir_all(&out_dir);
    let dest_str = out_dir.to_string_lossy().to_string();

    let volumes = lines(run(&["volume", "ls", "--format", "{{.Name}}"]));
    let mut count = 0usize;
    let mut bytes = 0u64;
    let mut errors = Vec::new();

    for v in &volumes {
        let mount_vol = format!("{v}:/from");
        let mount_out = format!("{dest_str}:/to");
        let tar = format!("/to/{v}.tar.gz");
        let result = Command::new("docker")
            .args([
                "run", "--rm", "-v", &mount_vol, "-v", &mount_out, "alpine", "tar", "czf",
                &tar, "-C", "/from", ".",
            ])
            .output();
        match result {
            Ok(o) if o.status.success() => {
                count += 1;
                if let Ok(m) = fs::metadata(out_dir.join(format!("{v}.tar.gz"))) {
                    bytes += m.len();
                }
            }
            Ok(o) => errors.push(format!(
                "{v}: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            )),
            Err(e) => errors.push(format!("{v}: {e}")),
        }
    }

    DockerBackupResult {
        volumes: count,
        bytes,
        dest: dest_str,
        errors,
    }
}
