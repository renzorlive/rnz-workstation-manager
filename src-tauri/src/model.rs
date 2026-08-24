use serde::{Deserialize, Serialize};

/// A single junk category and its size (e.g. "node_modules" -> 4.2 GB).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JunkEntry {
    pub name: String,
    pub bytes: u64,
}

/// What a discovered item actually is. Classification happens per item, never
/// by the workspace's name/path — a folder in Downloads can be a real PROJECT
/// while a folder in AppData is CACHE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemType {
    Project,
    ProjectContainer,
    Cache,
    DependencyStore,
    BuildArtifact,
    ApplicationData,
    SystemData,
    Archive,
    File,
    Unknown,
}

impl Default for ItemType {
    fn default() -> Self {
        ItemType::Unknown
    }
}

impl ItemType {
    /// Counts toward the headline "projects" number (real, git-auditable code).
    pub fn is_project(self) -> bool {
        matches!(self, ItemType::Project)
    }
    /// Real user code (a project or a container of projects) — not noise.
    pub fn is_real(self) -> bool {
        matches!(self, ItemType::Project | ItemType::ProjectContainer)
    }
    pub fn as_str(self) -> &'static str {
        match self {
            ItemType::Project => "project",
            ItemType::ProjectContainer => "project_container",
            ItemType::Cache => "cache",
            ItemType::DependencyStore => "dependency_store",
            ItemType::BuildArtifact => "build_artifact",
            ItemType::ApplicationData => "application_data",
            ItemType::SystemData => "system_data",
            ItemType::Archive => "archive",
            ItemType::File => "file",
            ItemType::Unknown => "unknown",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "project" => ItemType::Project,
            "project_container" => ItemType::ProjectContainer,
            "cache" => ItemType::Cache,
            "dependency_store" => ItemType::DependencyStore,
            "build_artifact" => ItemType::BuildArtifact,
            "application_data" => ItemType::ApplicationData,
            "system_data" => ItemType::SystemData,
            "archive" => ItemType::Archive,
            "file" => ItemType::File,
            _ => ItemType::Unknown,
        }
    }
}

/// A single detected software project plus its computed metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: i64,
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub item_type: ItemType,
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
    /// Real projects only (ItemType::Project).
    pub project_count: usize,
    /// Everything the scanner surfaced (projects + containers + caches + …).
    pub discovered_items: usize,
    pub container_count: usize,
    /// Cache + DependencyStore.
    pub cache_count: usize,
    pub appdata_count: usize,
    /// Archive + File + BuildArtifact + SystemData + Unknown.
    pub other_count: usize,
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

// ---------------------------------------------------------------------------
// Pre-reinstall audit (read-only)
// ---------------------------------------------------------------------------

/// Read-only Git state for one repository. All fields are safe to serialize:
/// remote URLs are credential-redacted before they reach here.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitInfo {
    pub is_repo: bool,
    /// Branch name, or "(detached)" / "" when unknown.
    pub branch: String,
    pub detached: bool,
    /// Short HEAD sha ("" when the repo has no commits yet).
    pub head: String,
    /// Last commit time (unix secs, 0 = unknown / no commits).
    pub last_commit: i64,
    pub dirty: bool,
    /// Tracked files with staged/unstaged changes.
    pub modified: u32,
    pub untracked: u32,
    pub ahead: u32,
    pub behind: u32,
    pub has_upstream: bool,
    /// Credential-redacted remote URLs (deduped).
    pub remotes: Vec<String>,
    pub has_remote: bool,
}

/// A detected environment/secret file. The value/content is NEVER captured —
/// only the variable count and whether Git is tracking it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvFile {
    pub path: String,
    pub name: String,
    pub var_count: u32,
    pub tracked_by_git: bool,
}

/// Per-project audit row: Git state, env files and derived severity/issues.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectAudit {
    pub name: String,
    pub path: String,
    pub stack: Vec<String>,
    pub size_bytes: u64,
    pub git: GitInfo,
    pub env_files: Vec<EnvFile>,
    /// "critical" | "warning" | "ok".
    pub severity: String,
    pub issues: Vec<String>,
}

/// One installed developer tool and its version (machine-level inventory).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareItem {
    pub name: String,
    pub version: String,
    pub found: bool,
}

/// The consolidated pre-reinstall audit report (§16-18 of the spec).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditReport {
    pub generated_at: i64,
    pub projects: Vec<ProjectAudit>,
    pub software: Vec<SoftwareItem>,
    // ---- Discovery buckets (all scanned items) ----
    pub discovered_items: usize,
    pub real_projects: usize,
    pub containers: usize,
    pub caches: usize,
    pub application_data: usize,
    pub other_items: usize,
    pub discovery_warnings: Vec<String>,
    // ---- Git counters (real projects only) ----
    pub total_projects: usize,
    pub git_repos: usize,
    pub not_git: usize,
    pub dirty: usize,
    pub no_remote: usize,
    pub unpushed: usize,
    pub env_files_total: usize,
    pub tracked_secrets: usize,
    pub critical: Vec<String>,
    pub warnings: Vec<String>,
    pub safe_to_reinstall: bool,
}
