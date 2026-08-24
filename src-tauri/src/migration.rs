//! Migration discovery (read-only). Finds the system-level things a Windows
//! reinstall would lose that project/asset discovery doesn't see: browser
//! profiles, crypto wallets, VMs, native databases and WSL distros. Nothing is
//! modified and no file contents are read — only presence, counts and sizes.

use std::path::Path;
use std::process::Command;

use walkdir::WalkDir;

use crate::model::{MigrationDiscovery, MigrationItem, MigrationStatus};

/// Known browser crypto-wallet extension IDs → display name. A browser wallet
/// means the seed phrase is the ONLY reliable backup — a hard blocker.
const WALLET_IDS: &[(&str, &str)] = &[
    ("nkbihfbeogaeaoehlefnkodbefgpgknn", "MetaMask"),
    ("bfnaelmomeimhlpmgjnjophhpkkoljpa", "Phantom"),
    ("hnfanknocfeofbddgcijnmhnfnkdnaad", "Coinbase Wallet"),
    ("egjidjbpglichdcondbcbdnbeeppgdph", "Trust Wallet"),
    ("fhbohimaelbohpjbbldcngcnapndodjp", "Binance Web3"),
    ("aholpfdialjgjfhomihkjbmgjidlcdno", "Exodus"),
];

/// Chromium-family browsers to inspect: (display name, User Data dir under LOCALAPPDATA).
const BROWSERS: &[(&str, &[&str])] = &[
    ("Chrome", &["Google", "Chrome", "User Data"]),
    ("Brave", &["BraveSoftware", "Brave-Browser", "User Data"]),
    ("Edge", &["Microsoft", "Edge", "User Data"]),
];

/// Structured-data file extensions worth migrating (scrapes, exports, datasets).
const DATA_EXTS: &[&str] = &[
    "csv", "tsv", "json", "jsonl", "ndjson", "parquet", "sqlite", "sqlite3", "db", "xlsx", "sql",
];

pub fn discover(now: i64) -> MigrationDiscovery {
    let mut items: Vec<MigrationItem> = Vec::new();

    browsers_and_wallets(&mut items);
    vmware_vms(&mut items);
    native_databases(&mut items);
    wsl_distros(&mut items);
    flat_file_assets(&mut items);
    passkeys_notice(&mut items);

    items.sort_by_key(|i| (i.status.rank(), std::cmp::Reverse(i.size_bytes)));

    let blockers = items.iter().filter(|i| i.status == MigrationStatus::Blocker).count();
    let manual_actions = items.iter().filter(|i| i.status == MigrationStatus::ManualAction).count();
    let not_backed_up = items.iter().filter(|i| i.status == MigrationStatus::NotBackedUp).count();

    MigrationDiscovery {
        generated_at: now,
        items,
        blockers,
        manual_actions,
        not_backed_up,
    }
}

fn item(
    category: &str,
    name: &str,
    detail: String,
    path: String,
    size_bytes: u64,
    status: MigrationStatus,
    action: &str,
) -> MigrationItem {
    MigrationItem {
        category: category.to_string(),
        name: name.to_string(),
        detail,
        path,
        size_bytes,
        status,
        action: action.to_string(),
    }
}

/// Enumerate browser profiles + detect crypto wallets. Passwords/cookies are
/// DPAPI-encrypted (won't restore on a new machine), so we flag sync; bookmarks
/// are plaintext and backupable.
fn browsers_and_wallets(out: &mut Vec<MigrationItem>) {
    let local = match dirs::data_local_dir() {
        Some(p) => p,
        None => return,
    };
    for (browser, segs) in BROWSERS {
        let user_data = segs.iter().fold(local.clone(), |p, s| p.join(s));
        if !user_data.is_dir() {
            continue;
        }
        let mut profiles = 0usize;
        if let Ok(entries) = std::fs::read_dir(&user_data) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !entry.path().is_dir() || !(name == "Default" || name.starts_with("Profile")) {
                    continue;
                }
                profiles += 1;
                // Detect wallet extensions in this profile.
                let ext_dir = entry.path().join("Extensions");
                for (id, wname) in WALLET_IDS {
                    if ext_dir.join(id).is_dir() {
                        out.push(item(
                            "wallet",
                            wname,
                            format!("{browser} · profile {name}"),
                            ext_dir.join(id).to_string_lossy().to_string(),
                            0,
                            MigrationStatus::Blocker,
                            "Back up the seed/recovery phrase OFFLINE — a profile copy is not a reliable wallet backup.",
                        ));
                    }
                }
            }
        }
        if profiles > 0 {
            out.push(item(
                "browser",
                browser,
                format!("{profiles} profile(s) — bookmarks backupable; passwords/cookies need account sync"),
                user_data.to_string_lossy().to_string(),
                0,
                MigrationStatus::NotBackedUp,
                "Back up bookmarks; verify Sync is on for the profiles that matter (passwords won't decrypt on a new machine).",
            ));
        }
    }
}

/// VMware VMs from the inventory file (each is like a whole machine).
fn vmware_vms(out: &mut Vec<MigrationItem>) {
    let inv = match dirs::config_dir() {
        Some(roaming) => roaming.join("VMware").join("inventory.vmls"),
        None => return,
    };
    let text = match std::fs::read_to_string(&inv) {
        Ok(t) => t,
        Err(_) => return,
    };
    let mut seen = std::collections::BTreeSet::new();
    for line in text.lines() {
        // vmlist<N>.config = "C:\...\Name.vmx"
        if let Some(vmx) = extract_quoted_ending(line, ".vmx") {
            if !seen.insert(vmx.clone()) {
                continue;
            }
            let path = Path::new(&vmx);
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| vmx.clone());
            let folder = path.parent().unwrap_or(path);
            let size = capped_dir_size(folder);
            out.push(item(
                "vm",
                &name,
                format!("VMware VM — {}", if path.exists() { "present" } else { "path in inventory" }),
                vmx,
                size,
                MigrationStatus::NotBackedUp,
                "Copy the whole VM folder (.vmx + .vmdk) to external storage if you still need it.",
            ));
        }
    }
}

/// Native (non-Docker) database engines currently running, by process name.
fn native_databases(out: &mut Vec<MigrationItem>) {
    let procs = running_processes();
    let dbs: &[(&str, &str, &str)] = &[
        ("mongod", "MongoDB", "mongodump --out <dest>"),
        ("postgres", "PostgreSQL", "pg_dump / pg_dumpall to <dest>"),
        ("mysqld", "MySQL", "mysqldump --all-databases > <dest>"),
        ("mariadbd", "MariaDB", "mysqldump --all-databases > <dest>"),
        ("redis-server", "Redis", "copy dump.rdb / BGSAVE"),
        ("sqlservr", "SQL Server", "BACKUP DATABASE for each DB"),
    ];
    for (proc, name, how) in dbs {
        if procs.iter().any(|p| p == proc) {
            out.push(item(
                "database",
                name,
                "Native database running — its data is NOT in a Docker volume".to_string(),
                String::new(),
                0,
                MigrationStatus::NotBackedUp,
                how,
            ));
        }
    }
}

/// Installed WSL distributions (excluding Docker's managed distros).
fn wsl_distros(out: &mut Vec<MigrationItem>) {
    let output = match Command::new("wsl").args(["--list", "--quiet"]).output() {
        Ok(o) if o.status.success() => o,
        _ => return,
    };
    for line in decode_utf16(&output.stdout).lines() {
        let name = line.trim().trim_end_matches('\0').trim();
        if name.is_empty() || name.eq_ignore_ascii_case("docker-desktop") || name.eq_ignore_ascii_case("docker-desktop-data") {
            continue;
        }
        out.push(item(
            "wsl",
            name,
            "WSL distribution — repos/data/dotfiles inside are lost on reinstall".to_string(),
            String::new(),
            0,
            MigrationStatus::NotBackedUp,
            &format!("wsl --export {name} <dest>\\{name}.tar"),
        ));
    }
}

/// Large structured-data folders NOT under git — the gap that hides scrapes,
/// datasets and exports outside any tracked project (e.g. an 11k-row cars.csv
/// sitting in Documents). Read-only: counts data files and sums their size.
fn flat_file_assets(out: &mut Vec<MigrationItem>) {
    const MIN_BYTES: u64 = 20 * 1024 * 1024; // 20 MB — ignore small config/data
    let roots = [dirs::document_dir(), dirs::download_dir(), dirs::desktop_dir()];
    for root in roots.into_iter().flatten() {
        let entries = match std::fs::read_dir(&root) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() || dir.join(".git").exists() {
                continue; // non-dir, or a tracked repo (git-bundle backup covers it)
            }
            let (bytes, files) = data_dir_size(&dir);
            if bytes < MIN_BYTES {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            out.push(item(
                "data-asset",
                &name,
                format!("{files} data file(s), not in git — unversioned dataset/scrape/export"),
                dir.to_string_lossy().to_string(),
                bytes,
                MigrationStatus::NotBackedUp,
                "Copy this folder to external storage (or git-init + push) — it may exist nowhere else.",
            ));
        }
    }
}

/// Passkeys can't be enumerated or copied (device-bound); always advise.
fn passkeys_notice(out: &mut Vec<MigrationItem>) {
    out.push(item(
        "passkeys",
        "Passkeys / Windows Hello",
        "Device-bound credentials — they cannot be backed up as files".to_string(),
        String::new(),
        0,
        MigrationStatus::ManualAction,
        "For every account with a passkey, ensure it syncs (Google/1Password/iCloud) OR you have an alternate login + recovery codes.",
    ));
}

// --- helpers ---------------------------------------------------------------

/// Running process base names (lower-cased, no extension) via `tasklist`.
fn running_processes() -> Vec<String> {
    let out = match Command::new("tasklist").args(["/fo", "csv", "/nh"]).output() {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split('"').nth(1).map(|s| s.to_string()))
        .map(|name| {
            name.to_lowercase()
                .trim_end_matches(".exe")
                .to_string()
        })
        .collect()
}

/// Extract the quoted path on `line` if it ends with `suffix` (case-insensitive).
fn extract_quoted_ending(line: &str, suffix: &str) -> Option<String> {
    let start = line.find('"')?;
    let rest = &line[start + 1..];
    let end = rest.find('"')?;
    let val = &rest[..end];
    if val.to_lowercase().ends_with(suffix) {
        Some(val.to_string())
    } else {
        None
    }
}

/// Decode UTF-16LE bytes (wsl.exe output) to a String, lossily.
fn decode_utf16(bytes: &[u8]) -> String {
    let u16s: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&u16s)
}

/// Recursive size capped at 50k files (VM/disk folders can be huge).
fn capped_dir_size(dir: &Path) -> u64 {
    let mut bytes = 0u64;
    let mut files = 0u64;
    for entry in WalkDir::new(dir).follow_links(false).into_iter().flatten() {
        if entry.file_type().is_file() {
            if let Ok(m) = entry.metadata() {
                bytes += m.len();
            }
            files += 1;
            if files >= 50_000 {
                break;
            }
        }
    }
    bytes
}

/// True if `path` has a structured-data extension we care to migrate.
fn is_data_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| DATA_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// (bytes, count) of DATA files under `dir`, pruning regenerable/noise dirs,
/// capped at 50k files.
// ponytail: naive full-walk under each candidate; the 50k cap bounds it. If a
// huge Downloads subtree ever makes this slow, add a max-depth or size budget.
fn data_dir_size(dir: &Path) -> (u64, u64) {
    const PRUNE: &[&str] = &[
        "node_modules", ".venv", "venv", "__pycache__", ".cache", ".git", ".next", "dist", "build",
    ];
    let mut bytes = 0u64;
    let mut files = 0u64;
    let walk = WalkDir::new(dir).follow_links(false).into_iter().filter_entry(|e| {
        !(e.file_type().is_dir()
            && e.file_name().to_str().map(|n| PRUNE.contains(&n)).unwrap_or(false))
    });
    for entry in walk.flatten() {
        if entry.file_type().is_file() && is_data_file(entry.path()) {
            if let Ok(m) = entry.metadata() {
                bytes += m.len();
            }
            files += 1;
            if files >= 50_000 {
                break;
            }
        }
    }
    (bytes, files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_data_files() {
        assert!(is_data_file(Path::new(r"C:\x\output\cars.csv")));
        assert!(is_data_file(Path::new("data.JSONL"))); // case-insensitive
        assert!(is_data_file(Path::new("db.sqlite3")));
        assert!(!is_data_file(Path::new("pipeline.py")));
        assert!(!is_data_file(Path::new("README.md")));
        assert!(!is_data_file(Path::new("noext")));
    }

    #[test]
    fn extracts_vmx_path() {
        let line = r#"vmlist1.config = "C:\Users\x\CAR SOFTWARE.vmx""#;
        assert_eq!(
            extract_quoted_ending(line, ".vmx").as_deref(),
            Some(r"C:\Users\x\CAR SOFTWARE.vmx")
        );
        assert_eq!(extract_quoted_ending(r#"index0 = "5""#, ".vmx"), None);
    }

    #[test]
    fn decodes_utf16() {
        // "Ubuntu" in UTF-16LE.
        let bytes = [0x55, 0, 0x62, 0, 0x75, 0, 0x6e, 0, 0x74, 0, 0x75, 0];
        assert_eq!(decode_utf16(&bytes), "Ubuntu");
    }

    #[test]
    fn wallet_ids_cover_metamask() {
        assert!(WALLET_IDS.iter().any(|(_, n)| *n == "MetaMask"));
    }
}
