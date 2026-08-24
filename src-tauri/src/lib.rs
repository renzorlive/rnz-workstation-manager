//! RNZ Workstation Manager — Tauri backend entry point.

mod assets;
mod audit;
mod backup;
mod classify;
mod db;
mod detector;
mod docker;
mod git;
mod health;
mod junk;
mod migration;
mod model;
mod recovery;
mod scanner;
mod secrets;
mod software;

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::Manager;

use model::{
    AssetInventory, AuditReport, BackupResult, DockerBackupResult, DockerStatus, MigrationDiscovery,
    Project, Readiness, Snapshot, Workspace, WorkspaceStats,
};

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Resolve (and create) the SQLite path inside the app data directory.
fn db_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("cannot resolve app data dir: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create data dir: {e}"))?;
    Ok(dir.join("rnz.db"))
}

fn open_db(app: &tauri::AppHandle) -> Result<rusqlite::Connection, String> {
    let dbp = db_path(app)?;
    db::open(&dbp).map_err(|e| format!("db open failed: {e}"))
}

// ---------------------------------------------------------------------------
// Workspace commands
// ---------------------------------------------------------------------------

/// Create (or update) a saved workspace root.
#[tauri::command]
fn add_workspace(
    app: tauri::AppHandle,
    name: String,
    path: String,
    color: String,
) -> Result<Workspace, String> {
    if !PathBuf::from(&path).is_dir() {
        return Err(format!("not a directory: {path}"));
    }
    let conn = open_db(&app)?;
    db::add_workspace(&conn, &name, &path, &color, now_secs()).map_err(|e| e.to_string())
}

/// Remove a workspace and its stored projects.
#[tauri::command]
fn delete_workspace(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    let conn = open_db(&app)?;
    db::delete_workspace(&conn, id).map_err(|e| e.to_string())
}

/// List all workspaces with aggregated stats.
#[tauri::command]
fn list_workspaces(app: tauri::AppHandle) -> Result<Vec<WorkspaceStats>, String> {
    let conn = open_db(&app)?;
    db::all_workspace_stats(&conn, now_secs()).map_err(|e| e.to_string())
}

/// Projects belonging to a workspace (no rescan).
#[tauri::command]
fn workspace_projects(app: tauri::AppHandle, id: i64) -> Result<Vec<Project>, String> {
    let conn = open_db(&app)?;
    db::projects_for_workspace(&conn, id).map_err(|e| e.to_string())
}

/// Snapshot history for a workspace, newest first.
#[tauri::command]
fn workspace_history(app: tauri::AppHandle, id: i64) -> Result<Vec<Snapshot>, String> {
    let conn = open_db(&app)?;
    db::workspace_history(&conn, id, 60).map_err(|e| e.to_string())
}

/// All projects across every workspace (for the overview aggregates).
#[tauri::command]
fn list_all_projects(app: tauri::AppHandle) -> Result<Vec<Project>, String> {
    let conn = open_db(&app)?;
    db::all_projects(&conn).map_err(|e| e.to_string())
}

/// Search projects across every workspace by name or path.
#[tauri::command]
fn search_projects(app: tauri::AppHandle, query: String) -> Result<Vec<Project>, String> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let conn = open_db(&app)?;
    db::search_projects(&conn, &query).map_err(|e| e.to_string())
}

/// Rescan a workspace from disk and return its refreshed stats.
#[tauri::command]
async fn scan_workspace(app: tauri::AppHandle, id: i64) -> Result<WorkspaceStats, String> {
    let now = now_secs();
    let mut conn = open_db(&app)?;
    let ws = db::workspace_by_id(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("workspace {id} not found"))?;

    let root = PathBuf::from(&ws.path);
    if !root.is_dir() {
        return Err(format!("workspace path missing: {}", ws.path));
    }

    // Skip sub-folders that are themselves registered workspaces, so an outer
    // workspace never steals a nested one's projects.
    let excludes: Vec<PathBuf> = db::all_workspaces(&conn)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|w| w.id != id)
        .map(|w| PathBuf::from(w.path))
        .collect();

    let projects = tauri::async_runtime::spawn_blocking(move || scanner::scan_excluding(&root, &excludes))
        .await
        .map_err(|e| format!("scan task failed: {e}"))?;

    db::replace_workspace_projects(&mut conn, id, &projects, now).map_err(|e| e.to_string())?;
    db::touch_workspace_scanned(&conn, id, now).map_err(|e| e.to_string())?;

    // Record a point-in-time snapshot for trend history.
    let snap = db::current_snapshot(&conn, id, now).map_err(|e| e.to_string())?;
    db::insert_snapshot(&conn, &snap).map_err(|e| e.to_string())?;

    let ws = db::workspace_by_id(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or("workspace vanished")?;
    db::workspace_stats(&conn, ws, now).map_err(|e| e.to_string())
}

/// Rescan every workspace and return all refreshed stats.
#[tauri::command]
async fn scan_all_workspaces(app: tauri::AppHandle) -> Result<Vec<WorkspaceStats>, String> {
    let conn = open_db(&app)?;
    let workspaces = db::all_workspaces(&conn).map_err(|e| e.to_string())?;
    drop(conn);

    for ws in workspaces {
        if PathBuf::from(&ws.path).is_dir() {
            scan_workspace(app.clone(), ws.id).await?;
        }
    }

    let conn = open_db(&app)?;
    db::all_workspace_stats(&conn, now_secs()).map_err(|e| e.to_string())
}

/// Flag or unflag a project as "not a project" (persists across rescans).
#[tauri::command]
fn set_project_ignored(app: tauri::AppHandle, path: String, ignored: bool) -> Result<(), String> {
    let conn = open_db(&app)?;
    db::set_ignored(&conn, &path, ignored).map_err(|e| e.to_string())
}

/// Reinstall-readiness checklist (which dev config artifacts exist).
#[tauri::command]
fn recovery_readiness() -> Readiness {
    recovery::readiness()
}

/// Build a recovery zip in `dir` named `filename`, bundling config artifacts
/// plus a project inventory. Returns the full path written.
#[tauri::command]
fn create_recovery_pack(
    app: tauri::AppHandle,
    dir: String,
    filename: String,
) -> Result<String, String> {
    let conn = open_db(&app)?;
    let projects = db::all_projects(&conn).map_err(|e| e.to_string())?;
    let inventory: Vec<serde_json::Value> = projects
        .iter()
        .filter(|p| p.item_type.is_real())
        .map(|p| {
            serde_json::json!({
                "name": p.name,
                "path": p.path,
                "stack": p.stack,
                "size_bytes": p.size_bytes,
                "junk_bytes": p.junk_bytes,
                "git_present": p.git_present,
                "has_readme": p.has_readme,
                "last_activity": p.last_activity,
                "health_score": p.health_score,
            })
        })
        .collect();
    let json = serde_json::to_string_pretty(&inventory).map_err(|e| e.to_string())?;
    let docker_json = serde_json::to_string_pretty(&docker::status()).unwrap_or_else(|_| "{}".into());

    let extras = vec![
        ("project-inventory.json", json),
        ("docker-manifest.json", docker_json),
    ];
    let dest = std::path::PathBuf::from(&dir).join(&filename);
    recovery::build_pack(&dest, &extras).map_err(|e| format!("recovery pack failed: {e}"))?;
    Ok(dest.to_string_lossy().to_string())
}

/// Resolve the user's home directory (for the discovery default).
#[tauri::command]
fn home_dir() -> String {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Discover projects under a root folder (defaults to the home dir). This does
/// not persist anything — it just helps the user find where their projects are.
#[tauri::command]
async fn discover_projects(root: String) -> Result<Vec<Project>, String> {
    let root_path = if root.trim().is_empty() {
        dirs::home_dir().ok_or("cannot resolve home directory")?
    } else {
        PathBuf::from(&root)
    };
    if !root_path.is_dir() {
        return Err(format!("not a directory: {}", root_path.to_string_lossy()));
    }
    tauri::async_runtime::spawn_blocking(move || scanner::scan(&root_path))
        .await
        .map_err(|e| format!("discovery failed: {e}"))
}

/// Back up every (non-ignored) project of a workspace into `dest`, copying
/// source + .git but skipping regenerable dependency/build directories.
#[tauri::command]
async fn backup_workspace(
    app: tauri::AppHandle,
    id: i64,
    dest: String,
) -> Result<BackupResult, String> {
    let conn = open_db(&app)?;
    let ws = db::workspace_by_id(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("workspace {id} not found"))?;
    let projects = db::projects_for_workspace(&conn, id).map_err(|e| e.to_string())?;
    drop(conn);

    let dest_root = PathBuf::from(&dest).join(&ws.name);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut files = 0u64;
        let mut bytes = 0u64;
        let mut count = 0usize;
        for p in projects {
            if p.ignored {
                continue;
            }
            let src = PathBuf::from(&p.path);
            if !src.is_dir() {
                continue;
            }
            let target = dest_root.join(&p.name);
            if let Ok((f, b)) = backup::copy_clean(&src, &target) {
                files += f;
                bytes += b;
                count += 1;
            }
        }
        BackupResult {
            projects: count,
            files,
            bytes_copied: bytes,
            dest: dest_root.to_string_lossy().to_string(),
        }
    })
    .await
    .map_err(|e| format!("backup task failed: {e}"))?;
    Ok(result)
}

/// Back up a single project into `dest`, skipping regenerable junk.
#[tauri::command]
async fn backup_project(
    path: String,
    name: String,
    dest: String,
) -> Result<BackupResult, String> {
    let src = PathBuf::from(&path);
    if !src.is_dir() {
        return Err(format!("not a directory: {path}"));
    }
    let target = PathBuf::from(&dest).join(&name);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let (files, bytes) = backup::copy_clean(&src, &target).unwrap_or((0, 0));
        BackupResult {
            projects: 1,
            files,
            bytes_copied: bytes,
            dest: target.to_string_lossy().to_string(),
        }
    })
    .await
    .map_err(|e| format!("backup task failed: {e}"))?;
    Ok(result)
}

/// Snapshot of the local Docker state (containers/images/volumes/networks).
#[tauri::command]
fn docker_status() -> DockerStatus {
    docker::status()
}

/// Export every named Docker volume into `dest/docker-volumes/*.tar.gz`.
#[tauri::command]
async fn backup_docker_volumes(dest: String) -> Result<DockerBackupResult, String> {
    let path = PathBuf::from(&dest);
    if !path.is_dir() {
        return Err(format!("not a directory: {dest}"));
    }
    tauri::async_runtime::spawn_blocking(move || docker::export_volumes(&path))
        .await
        .map_err(|e| format!("docker backup failed: {e}"))
}

// ---------------------------------------------------------------------------
// Pre-reinstall audit (read-only)
// ---------------------------------------------------------------------------

/// Run the read-only pre-reinstall audit over every scanned project plus the
/// machine software inventory. Does not modify anything on disk.
#[tauri::command]
async fn run_audit(app: tauri::AppHandle) -> Result<AuditReport, String> {
    let conn = open_db(&app)?;
    let projects = db::all_projects(&conn).map_err(|e| e.to_string())?;
    drop(conn);
    let now = now_secs();
    tauri::async_runtime::spawn_blocking(move || audit::run(projects, now))
        .await
        .map_err(|e| format!("audit failed: {e}"))
}

/// Read-only workstation asset discovery: dotfiles/dirs under home plus AppData
/// app folders, classified for reinstall recovery. Never reads file contents.
#[tauri::command]
async fn discover_assets() -> Result<AssetInventory, String> {
    let now = now_secs();
    tauri::async_runtime::spawn_blocking(move || assets::discover(now))
        .await
        .map_err(|e| format!("asset discovery failed: {e}"))
}

/// Read-only migration discovery: browsers/wallets, VMs, native databases and
/// WSL distros a reinstall would lose, each with a backup-readiness status.
#[tauri::command]
async fn discover_migration() -> Result<MigrationDiscovery, String> {
    let now = now_secs();
    tauri::async_runtime::spawn_blocking(move || migration::discover(now))
        .await
        .map_err(|e| format!("migration discovery failed: {e}"))
}

/// Write the audit report to `dir` as both `audit.json` and `audit-report.md`.
/// Returns the directory written to.
#[tauri::command]
fn export_audit(dir: String, report: AuditReport) -> Result<String, String> {
    let dir_path = PathBuf::from(&dir);
    if !dir_path.is_dir() {
        return Err(format!("not a directory: {dir}"));
    }
    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    std::fs::write(dir_path.join("audit.json"), json).map_err(|e| e.to_string())?;
    std::fs::write(dir_path.join("audit-report.md"), audit::to_markdown(&report))
        .map_err(|e| e.to_string())?;
    Ok(dir)
}

// ---------------------------------------------------------------------------
// External openers
// ---------------------------------------------------------------------------

/// Reveal a folder in the OS file explorer.
#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    use std::process::Command;
    let result = if cfg!(target_os = "windows") {
        Command::new("explorer").arg(&path).spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(&path).spawn()
    } else {
        Command::new("xdg-open").arg(&path).spawn()
    };
    result.map(|_| ()).map_err(|e| format!("cannot open folder: {e}"))
}

/// Open a folder in VS Code via the `code` CLI.
#[tauri::command]
fn open_vscode(path: String) -> Result<(), String> {
    use std::process::Command;
    // On Windows `code` is a .cmd shim, so it must be launched through cmd.
    let result = if cfg!(target_os = "windows") {
        Command::new("cmd").args(["/C", "code", &path]).spawn()
    } else {
        Command::new("code").arg(&path).spawn()
    };
    result
        .map(|_| ())
        .map_err(|e| format!("cannot open VS Code (is the 'code' command installed?): {e}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            add_workspace,
            delete_workspace,
            list_workspaces,
            workspace_projects,
            workspace_history,
            list_all_projects,
            search_projects,
            scan_workspace,
            scan_all_workspaces,
            set_project_ignored,
            recovery_readiness,
            create_recovery_pack,
            home_dir,
            discover_projects,
            backup_workspace,
            backup_project,
            docker_status,
            backup_docker_volumes,
            run_audit,
            export_audit,
            discover_assets,
            discover_migration,
            open_folder,
            open_vscode
        ])
        .run(tauri::generate_context!())
        .expect("error while running RNZ Workstation Manager");
}
