export interface JunkEntry {
  name: string;
  bytes: number;
}

export interface Project {
  id: number;
  path: string;
  name: string;
  stack: string[];
  size_bytes: number;
  junk_bytes: number;
  node_modules_bytes: number;
  build_bytes: number;
  archive_bytes: number;
  junk_detail: JunkEntry[];
  git_present: boolean;
  has_readme: boolean;
  last_activity: number; // unix seconds, 0 = unknown
  health_score: number;
  confidence: number;
  workspace_id: number | null;
  ignored: boolean;
}

export interface Workspace {
  id: number;
  name: string;
  path: string;
  color: string;
  auto_scan_enabled: boolean;
  last_scanned: number; // unix seconds, 0 = never
}

export interface WorkspaceStats {
  workspace: Workspace;
  project_count: number;
  active_count: number;
  dormant_count: number;
  archived_count: number;
  total_size_bytes: number;
  total_junk_bytes: number;
  health_score: number;
  delta_projects: number | null;
  delta_junk_bytes: number | null;
  delta_health: number | null;
  junk_detail: JunkEntry[];
}

export interface Snapshot {
  id: number;
  workspace_id: number;
  scan_date: number; // unix seconds
  project_count: number;
  active_count: number;
  dormant_count: number;
  archived_count: number;
  total_size_bytes: number;
  junk_bytes: number;
  health_score: number;
}

export interface ReadinessItem {
  key: string;
  label: string;
  category: string;
  path: string;
  present: boolean;
  size_bytes: number;
  secret: boolean;
  essential: boolean;
}

export interface Readiness {
  score: number;
  items: ReadinessItem[];
}

export interface BackupResult {
  projects: number;
  files: number;
  bytes_copied: number;
  dest: string;
}

export interface DockerContainer {
  name: string;
  image: string;
  state: string;
}

export interface DockerStatus {
  available: boolean;
  running: boolean;
  containers: DockerContainer[];
  images: string[];
  volumes: string[];
  networks: string[];
}

export interface DockerBackupResult {
  volumes: number;
  bytes: number;
  dest: string;
  errors: string[];
}
