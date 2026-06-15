use serde::{Deserialize, Serialize};

/// A single junk category and its size (e.g. "node_modules" -> 4.2 GB).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JunkEntry {
    pub name: String,
    pub bytes: u64,
}

/// A single detected software project plus its computed metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub stack: Vec<String>,
    pub size_bytes: u64,
    pub junk_bytes: u64,
    pub node_modules_bytes: u64,
    pub build_bytes: u64,
    pub archive_bytes: u64,
    /// Per-category junk breakdown, largest first.
    pub junk_detail: Vec<JunkEntry>,
    pub git_present: bool,
    pub has_readme: bool,
    pub last_activity: i64,
    pub health_score: i32,
    pub confidence: i32,
    pub workspace_id: Option<i64>,
    pub ignored: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub color: String,
    pub auto_scan_enabled: bool,
    pub last_scanned: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceStats {
    pub workspace: Workspace,
    pub project_count: usize,
    pub active_count: usize,
    pub dormant_count: usize,
    pub archived_count: usize,
    pub total_size_bytes: u64,
    pub total_junk_bytes: u64,
    pub health_score: i32,
    pub delta_projects: Option<i64>,
    pub delta_junk_bytes: Option<i64>,
    pub delta_health: Option<i32>,
    /// Aggregated junk categories across the workspace's projects.
    pub junk_detail: Vec<JunkEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: i64,
    pub workspace_id: i64,
    pub scan_date: i64,
    pub project_count: usize,
    pub active_count: usize,
    pub dormant_count: usize,
    pub archived_count: usize,
    pub total_size_bytes: u64,
    pub junk_bytes: u64,
    pub health_score: i32,
}

/// A single workstation-recovery artifact (config/keys) and whether it exists.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessItem {
    pub key: String,
    pub label: String,
    pub category: String,
    pub path: String,
    pub present: bool,
    pub size_bytes: u64,
    /// Contains secrets (e.g. SSH private keys).
    pub secret: bool,
    /// Counts toward the readiness score.
    pub essential: bool,
}

/// Overall reinstall readiness: a score plus the per-artifact checklist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Readiness {
    pub score: i32,
    pub items: Vec<ReadinessItem>,
}

/// Result of a dev-aware backup copy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupResult {
    pub projects: usize,
    pub files: u64,
    pub bytes_copied: u64,
    pub dest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerContainer {
    pub name: String,
    pub image: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerStatus {
    pub available: bool,
    pub running: bool,
    pub containers: Vec<DockerContainer>,
    pub images: Vec<String>,
    pub volumes: Vec<String>,
    pub networks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerBackupResult {
    pub volumes: usize,
    pub bytes: u64,
    pub dest: String,
    pub errors: Vec<String>,
}
