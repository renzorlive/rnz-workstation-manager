//! Local SQLite storage for workspaces, projects and scan history.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use crate::model::{ItemType, JunkEntry, Project, Snapshot, Workspace, WorkspaceStats};

const DAY: i64 = 86_400;

/// Open (and migrate) the database at `path`.
pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspaces (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            name              TEXT NOT NULL,
            path              TEXT NOT NULL UNIQUE,
            color             TEXT NOT NULL DEFAULT '#4f8cff',
            auto_scan_enabled INTEGER NOT NULL DEFAULT 0,
            last_scanned      INTEGER NOT NULL DEFAULT 0,
            created_at        INTEGER NOT NULL DEFAULT 0,
            updated_at        INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS projects (
            id                 INTEGER PRIMARY KEY AUTOINCREMENT,
            path               TEXT NOT NULL UNIQUE,
            name               TEXT NOT NULL,
            item_type          TEXT NOT NULL DEFAULT 'unknown',
            stack              TEXT NOT NULL,
            size_bytes         INTEGER NOT NULL,
            junk_bytes         INTEGER NOT NULL,
            node_modules_bytes INTEGER NOT NULL DEFAULT 0,
            build_bytes        INTEGER NOT NULL DEFAULT 0,
            archive_bytes      INTEGER NOT NULL DEFAULT 0,
            junk_detail        TEXT NOT NULL DEFAULT '[]',
            git_present        INTEGER NOT NULL,
            has_readme         INTEGER NOT NULL DEFAULT 0,
            last_activity      INTEGER NOT NULL DEFAULT 0,
            health_score       INTEGER NOT NULL,
            confidence         INTEGER NOT NULL DEFAULT 0,
            workspace_id       INTEGER,
            created_at         INTEGER NOT NULL,
            updated_at         INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS scan_snapshots (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            workspace_id     INTEGER NOT NULL,
            scan_date        INTEGER NOT NULL,
            project_count    INTEGER NOT NULL,
            active_count     INTEGER NOT NULL,
            dormant_count    INTEGER NOT NULL,
            archived_count   INTEGER NOT NULL,
            total_size_bytes INTEGER NOT NULL,
            junk_bytes       INTEGER NOT NULL,
            health_score     INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS ignored_projects (
            path TEXT PRIMARY KEY
        );",
    )?;
    ensure_column(&conn, "projects", "workspace_id", "INTEGER")?;
    ensure_column(&conn, "projects", "confidence", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column(&conn, "projects", "junk_detail", "TEXT NOT NULL DEFAULT '[]'")?;
    ensure_column(&conn, "projects", "item_type", "TEXT NOT NULL DEFAULT 'unknown'")?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_projects_ws ON projects(workspace_id)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_snap_ws ON scan_snapshots(workspace_id, scan_date)",
        [],
    )?;
    Ok(conn)
}

fn ensure_column(conn: &Connection, table: &str, col: &str, decl: &str) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let existing: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    if !existing.iter().any(|c| c == col) {
        conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {col} {decl}"), [])?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Workspaces
// ---------------------------------------------------------------------------

pub fn add_workspace(
    conn: &Connection,
    name: &str,
    path: &str,
    color: &str,
    now: i64,
) -> rusqlite::Result<Workspace> {
    conn.execute(
        "INSERT INTO workspaces (name, path, color, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)
         ON CONFLICT(path) DO UPDATE SET name=?1, color=?3, updated_at=?4",
        params![name, path, color, now],
    )?;
    workspace_by_path(conn, path).map(|w| w.expect("just inserted"))
}

pub fn delete_workspace(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM projects WHERE workspace_id = ?1", params![id])?;
    conn.execute("DELETE FROM scan_snapshots WHERE workspace_id = ?1", params![id])?;
    conn.execute("DELETE FROM workspaces WHERE id = ?1", params![id])?;
    Ok(())
}

fn map_workspace(r: &rusqlite::Row) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: r.get(0)?,
        name: r.get(1)?,
        path: r.get(2)?,
        color: r.get(3)?,
        auto_scan_enabled: r.get::<_, i64>(4)? != 0,
        last_scanned: r.get(5)?,
    })
}

const WS_COLS: &str = "id, name, path, color, auto_scan_enabled, last_scanned";

pub fn workspace_by_path(conn: &Connection, path: &str) -> rusqlite::Result<Option<Workspace>> {
    conn.query_row(
        &format!("SELECT {WS_COLS} FROM workspaces WHERE path = ?1"),
        params![path],
        map_workspace,
    )
    .optional()
}

pub fn workspace_by_id(conn: &Connection, id: i64) -> rusqlite::Result<Option<Workspace>> {
    conn.query_row(
        &format!("SELECT {WS_COLS} FROM workspaces WHERE id = ?1"),
        params![id],
        map_workspace,
    )
    .optional()
}

pub fn all_workspaces(conn: &Connection) -> rusqlite::Result<Vec<Workspace>> {
    let mut stmt =
        conn.prepare(&format!("SELECT {WS_COLS} FROM workspaces ORDER BY name COLLATE NOCASE"))?;
    let rows = stmt.query_map([], map_workspace)?;
    rows.collect()
}

pub fn touch_workspace_scanned(conn: &Connection, id: i64, now: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE workspaces SET last_scanned = ?2, updated_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Ignore list
// ---------------------------------------------------------------------------

pub fn set_ignored(conn: &Connection, path: &str, ignored: bool) -> rusqlite::Result<()> {
    if ignored {
        conn.execute(
            "INSERT OR IGNORE INTO ignored_projects (path) VALUES (?1)",
            params![path],
        )?;
    } else {
        conn.execute("DELETE FROM ignored_projects WHERE path = ?1", params![path])?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Snapshots / history
// ---------------------------------------------------------------------------

pub fn current_snapshot(conn: &Connection, workspace_id: i64, now: i64) -> rusqlite::Result<Snapshot> {
    let projects = projects_for_workspace(conn, workspace_id)?;
    let mut total_size = 0u64;
    let mut total_junk = 0u64;
    let (mut active, mut dormant, mut archived) = (0usize, 0usize, 0usize);
    let mut health_sum = 0i64;
    let mut real_count = 0usize; // projects + containers (for size/junk/health)
    let mut count = 0usize; // real projects only (headline count + activity)
    for p in &projects {
        if p.ignored || !p.item_type.is_real() {
            continue;
        }
        // Size, junk and health span real code (projects + containers).
        real_count += 1;
        total_size += p.size_bytes;
        total_junk += p.junk_bytes;
        health_sum += p.health_score as i64;

        // Headline project count and activity buckets: real projects only.
        if !p.item_type.is_project() {
            continue;
        }
        count += 1;
        if p.last_activity == 0 {
            archived += 1;
        } else {
            let age = (now - p.last_activity) / DAY;
            if age <= 30 {
                active += 1;
            } else if age <= 180 {
                dormant += 1;
            } else {
                archived += 1;
            }
        }
    }
    let health = if real_count == 0 {
        100
    } else {
        (health_sum as f64 / real_count as f64).round() as i32
    };
    Ok(Snapshot {
        id: 0,
        workspace_id,
        scan_date: now,
        project_count: count,
        active_count: active,
        dormant_count: dormant,
        archived_count: archived,
        total_size_bytes: total_size,
        junk_bytes: total_junk,
        health_score: health,
    })
}

pub fn insert_snapshot(conn: &Connection, s: &Snapshot) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO scan_snapshots
            (workspace_id, scan_date, project_count, active_count, dormant_count,
             archived_count, total_size_bytes, junk_bytes, health_score)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            s.workspace_id,
            s.scan_date,
            s.project_count as i64,
            s.active_count as i64,
            s.dormant_count as i64,
            s.archived_count as i64,
            s.total_size_bytes as i64,
            s.junk_bytes as i64,
            s.health_score,
        ],
    )?;
    Ok(())
}

fn map_snapshot(r: &rusqlite::Row) -> rusqlite::Result<Snapshot> {
    Ok(Snapshot {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        scan_date: r.get(2)?,
        project_count: r.get::<_, i64>(3)? as usize,
        active_count: r.get::<_, i64>(4)? as usize,
        dormant_count: r.get::<_, i64>(5)? as usize,
        archived_count: r.get::<_, i64>(6)? as usize,
        total_size_bytes: r.get::<_, i64>(7)? as u64,
        junk_bytes: r.get::<_, i64>(8)? as u64,
        health_score: r.get(9)?,
    })
}

const SNAP_COLS: &str = "id, workspace_id, scan_date, project_count, active_count, \
    dormant_count, archived_count, total_size_bytes, junk_bytes, health_score";

pub fn workspace_history(conn: &Connection, workspace_id: i64, limit: i64) -> rusqlite::Result<Vec<Snapshot>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SNAP_COLS} FROM scan_snapshots WHERE workspace_id = ?1 \
         ORDER BY scan_date DESC, id DESC LIMIT ?2"
    ))?;
    let rows = stmt.query_map(params![workspace_id, limit], map_snapshot)?;
    rows.collect()
}

/// Sum junk categories across the (non-ignored) projects of a workspace.
fn aggregate_junk(projects: &[Project]) -> Vec<JunkEntry> {
    let mut agg: HashMap<String, u64> = HashMap::new();
    for p in projects {
        if p.ignored {
            continue;
        }
        for e in &p.junk_detail {
            *agg.entry(e.name.clone()).or_insert(0) += e.bytes;
        }
    }
    let mut out: Vec<JunkEntry> = agg
        .into_iter()
        .map(|(name, bytes)| JunkEntry { name, bytes })
        .collect();
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    out
}

pub fn workspace_stats(conn: &Connection, ws: Workspace, now: i64) -> rusqlite::Result<WorkspaceStats> {
    let cur = current_snapshot(conn, ws.id, now)?;
    let recent = workspace_history(conn, ws.id, 2)?;
    let (dp, dj, dh) = if recent.len() >= 2 {
        let (a, b) = (&recent[0], &recent[1]);
        (
            Some(a.project_count as i64 - b.project_count as i64),
            Some(a.junk_bytes as i64 - b.junk_bytes as i64),
            Some(a.health_score - b.health_score),
        )
    } else {
        (None, None, None)
    };
    let projects = projects_for_workspace(conn, ws.id)?;
    let (mut discovered, mut container, mut cache, mut appdata, mut other) = (0, 0, 0, 0, 0);
    for p in &projects {
        if p.ignored {
            continue;
        }
        discovered += 1;
        match p.item_type {
            ItemType::Project => {}
            ItemType::ProjectContainer => container += 1,
            ItemType::Cache | ItemType::DependencyStore => cache += 1,
            ItemType::ApplicationData => appdata += 1,
            _ => other += 1,
        }
    }
    Ok(WorkspaceStats {
        workspace: ws,
        project_count: cur.project_count,
        discovered_items: discovered,
        container_count: container,
        cache_count: cache,
        appdata_count: appdata,
        other_count: other,
        active_count: cur.active_count,
        dormant_count: cur.dormant_count,
        archived_count: cur.archived_count,
        total_size_bytes: cur.total_size_bytes,
        total_junk_bytes: cur.junk_bytes,
        health_score: cur.health_score,
        delta_projects: dp,
        delta_junk_bytes: dj,
        delta_health: dh,
        junk_detail: aggregate_junk(&projects),
    })
}

pub fn all_workspace_stats(conn: &Connection, now: i64) -> rusqlite::Result<Vec<WorkspaceStats>> {
    let mut out = Vec::new();
    for ws in all_workspaces(conn)? {
        out.push(workspace_stats(conn, ws, now)?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Projects
// ---------------------------------------------------------------------------

pub fn replace_workspace_projects(
    conn: &mut Connection,
    workspace_id: i64,
    projects: &[Project],
    now: i64,
) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM projects WHERE workspace_id = ?1", params![workspace_id])?;
    for p in projects {
        insert_project(&tx, p, Some(workspace_id), now)?;
    }
    tx.commit()
}

pub fn insert_project(
    conn: &Connection,
    p: &Project,
    workspace_id: Option<i64>,
    now: i64,
) -> rusqlite::Result<()> {
    let stack = p.stack.join(",");
    let junk_detail = serde_json::to_string(&p.junk_detail).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "INSERT INTO projects
            (path, name, item_type, stack, size_bytes, junk_bytes, node_modules_bytes,
             build_bytes, archive_bytes, junk_detail, git_present, has_readme,
             last_activity, health_score, confidence, workspace_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?17)
         ON CONFLICT(path) DO UPDATE SET
            name=?2, item_type=?3, stack=?4, size_bytes=?5, junk_bytes=?6,
            node_modules_bytes=?7, build_bytes=?8, archive_bytes=?9, junk_detail=?10,
            git_present=?11, has_readme=?12, last_activity=?13,
            health_score=?14, confidence=?15, workspace_id=?16, updated_at=?17",
        params![
            p.path,
            p.name,
            p.item_type.as_str(),
            stack,
            p.size_bytes as i64,
            p.junk_bytes as i64,
            p.node_modules_bytes as i64,
            p.build_bytes as i64,
            p.archive_bytes as i64,
            junk_detail,
            p.git_present as i64,
            p.has_readme as i64,
            p.last_activity,
            p.health_score,
            p.confidence,
            workspace_id,
            now,
        ],
    )?;
    Ok(())
}

fn map_project(r: &rusqlite::Row) -> rusqlite::Result<Project> {
    let stack: String = r.get(3)?;
    let junk_detail_json: String = r.get(9)?;
    let junk_detail: Vec<JunkEntry> = serde_json::from_str(&junk_detail_json).unwrap_or_default();
    let item_type: String = r.get(17)?;
    Ok(Project {
        id: r.get(0)?,
        path: r.get(1)?,
        name: r.get(2)?,
        item_type: crate::model::ItemType::parse(&item_type),
        stack: if stack.is_empty() {
            Vec::new()
        } else {
            stack.split(',').map(|s| s.to_string()).collect()
        },
        size_bytes: r.get::<_, i64>(4)? as u64,
        junk_bytes: r.get::<_, i64>(5)? as u64,
        node_modules_bytes: r.get::<_, i64>(6)? as u64,
        build_bytes: r.get::<_, i64>(7)? as u64,
        archive_bytes: r.get::<_, i64>(8)? as u64,
        junk_detail,
        git_present: r.get::<_, i64>(10)? != 0,
        has_readme: r.get::<_, i64>(11)? != 0,
        last_activity: r.get(12)?,
        health_score: r.get(13)?,
        confidence: r.get(14)?,
        workspace_id: r.get(15)?,
        ignored: r.get::<_, i64>(16)? != 0,
    })
}

const PROJ_SELECT: &str = "SELECT p.id, p.path, p.name, p.stack, p.size_bytes, p.junk_bytes, \
    p.node_modules_bytes, p.build_bytes, p.archive_bytes, p.junk_detail, p.git_present, \
    p.has_readme, p.last_activity, p.health_score, p.confidence, p.workspace_id, \
    (ip.path IS NOT NULL) AS ignored, p.item_type \
    FROM projects p LEFT JOIN ignored_projects ip ON ip.path = p.path";

pub fn projects_for_workspace(conn: &Connection, workspace_id: i64) -> rusqlite::Result<Vec<Project>> {
    let mut stmt = conn.prepare(&format!(
        "{PROJ_SELECT} WHERE p.workspace_id = ?1 ORDER BY p.size_bytes DESC"
    ))?;
    let rows = stmt.query_map(params![workspace_id], map_project)?;
    rows.collect()
}

pub fn all_projects(conn: &Connection) -> rusqlite::Result<Vec<Project>> {
    let mut stmt = conn.prepare(&format!("{PROJ_SELECT} ORDER BY p.size_bytes DESC"))?;
    let rows = stmt.query_map([], map_project)?;
    rows.collect()
}

pub fn search_projects(conn: &Connection, query: &str) -> rusqlite::Result<Vec<Project>> {
    let like = format!("%{}%", query.trim());
    let mut stmt = conn.prepare(&format!(
        "{PROJ_SELECT} WHERE (p.name LIKE ?1 OR p.path LIKE ?1) \
         AND p.item_type IN ('project', 'project_container') \
         ORDER BY p.size_bytes DESC LIMIT 300"
    ))?;
    let rows = stmt.query_map(params![like], map_project)?;
    rows.collect()
}

