import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  BackupResult,
  DockerBackupResult,
  DockerStatus,
  JunkEntry,
  Project,
  Readiness,
  WorkspaceStats,
} from "./types";
import {
  activityBucket,
  cleanupInfo,
  confidenceColor,
  formatActivity,
  formatBytes,
  healthColor,
  healthFactors,
  riskColor,
  type CleanupInfo,
  type Risk,
} from "./format";

const COLORS = ["#6f8cff", "#34d399", "#f5b34a", "#f8716a", "#9a6bff", "#22d3ee"];
const BACKUP_EXCLUDE = new Set([
  "node_modules", ".next", "dist", "build", "coverage", "target", "bin", "obj", "__pycache__", ".venv",
]);

type Section = "overview" | "workspaces" | "cleanup" | "backup" | "settings";

async function openFolder(path: string) {
  try { await invoke("open_folder", { path }); } catch (e) { console.error(e); }
}
async function openVSCode(path: string) {
  try { await invoke("open_vscode", { path }); } catch (e) { console.error(e); }
}
async function pickDir(): Promise<string | null> {
  const d = await open({ directory: true, multiple: false });
  return d && !Array.isArray(d) ? d : null;
}
function backupEstimate(s: WorkspaceStats): number {
  const excl = s.junk_detail.filter((j) => BACKUP_EXCLUDE.has(j.name)).reduce((a, b) => a + b.bytes, 0);
  return Math.max(0, s.total_size_bytes - excl);
}
function weightedHealth(ws: WorkspaceStats[]): number {
  let num = 0, den = 0;
  for (const w of ws) { num += w.health_score * w.project_count; den += w.project_count; }
  return den ? Math.round(num / den) : 100;
}
function parentOf(path: string): string {
  const i = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return i > 0 ? path.slice(0, i) : path;
}

function Score({ value, size = 34 }: { value: number; size?: number }) {
  const c = healthColor(value);
  return (
    <span className="score" style={{ width: size, height: size, color: c, borderColor: c }}>
      {value}
    </span>
  );
}

export default function App() {
  const [section, setSection] = useState<Section>("overview");
  const [workspaces, setWorkspaces] = useState<WorkspaceStats[]>([]);
  const [allProjects, setAllProjects] = useState<Project[]>([]);
  const [selWs, setSelWs] = useState<number | null>(null);
  const [wsProjects, setWsProjects] = useState<Project[]>([]);
  const [panel, setPanel] = useState<Project | null>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const [scanningAll, setScanningAll] = useState(false);
  const [scanningId, setScanningId] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const [ws, ps] = await Promise.all([
        invoke<WorkspaceStats[]>("list_workspaces"),
        invoke<Project[]>("list_all_projects"),
      ]);
      setWorkspaces(ws);
      setAllProjects(ps);
    } catch (e) { setError(String(e)); }
  }, []);

  useEffect(() => { load(); }, [load]);

  useEffect(() => {
    if (selWs == null) { setWsProjects([]); return; }
    invoke<Project[]>("workspace_projects", { id: selWs }).then(setWsProjects).catch((e) => setError(String(e)));
  }, [selWs, workspaces]);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setSearchOpen((v) => !v);
      } else if (e.key === "Escape") {
        setSearchOpen(false);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  async function scanAll() {
    setError(null); setScanningAll(true);
    try { setWorkspaces(await invoke<WorkspaceStats[]>("scan_all_workspaces")); await load(); }
    catch (e) { setError(String(e)); }
    finally { setScanningAll(false); }
  }
  async function scanWs(id: number) {
    setError(null); setScanningId(id);
    try {
      const stats = await invoke<WorkspaceStats>("scan_workspace", { id });
      setWorkspaces((ws) => ws.map((w) => (w.workspace.id === id ? stats : w)));
      if (selWs === id) setWsProjects(await invoke<Project[]>("workspace_projects", { id }));
      await load();
    } catch (e) { setError(String(e)); }
    finally { setScanningId(null); }
  }
  const toggleIgnore = useCallback(async (p: Project) => {
    try {
      await invoke("set_project_ignored", { path: p.path, ignored: !p.ignored });
      setPanel((dp) => (dp && dp.path === p.path ? { ...dp, ignored: !p.ignored } : dp));
      await load();
      if (selWs != null) setWsProjects(await invoke<Project[]>("workspace_projects", { id: selWs }));
    } catch (e) { setError(String(e)); }
  }, [load, selWs]);

  const wsName = useMemo(() => new Map(workspaces.map((w) => [w.workspace.id, w.workspace.name])), [workspaces]);

  const nav: { id: Section; label: string; icon: string }[] = [
    { id: "overview", label: "Overview", icon: "grid" },
    { id: "workspaces", label: "Workspaces", icon: "folder" },
    { id: "cleanup", label: "Cleanup", icon: "clean" },
    { id: "backup", label: "Backup", icon: "download" },
    { id: "settings", label: "Settings", icon: "sliders" },
  ];

  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">R</span>
          <span className="brand-name">Workstation</span>
        </div>
        <nav className="nav">
          {nav.map((n) => (
            <button
              key={n.id}
              className={`nav-item${section === n.id && !searchOpen ? " active" : ""}`}
              onClick={() => { setSection(n.id); setSelWs(null); }}
            >
              <span className="nav-icon"><Icon name={n.icon} /></span>
              {n.label}
            </button>
          ))}
          <button className="nav-item" onClick={() => setSearchOpen(true)}>
            <span className="nav-icon"><Icon name="search" /></span>
            Search
            <span className="kbd">⌘K</span>
          </button>
        </nav>
        <div className="sidebar-foot">
          <button className="btn" onClick={scanAll} disabled={scanningAll || workspaces.length === 0}>
            {scanningAll ? "Scanning…" : "Scan all"}
          </button>
        </div>
      </aside>

      <main className="main">
        {error && <div className="banner-error" onClick={() => setError(null)}>{error}</div>}

        {section === "overview" && (
          <Overview
            workspaces={workspaces}
            projects={allProjects}
            onOpenProject={setPanel}
            onGo={setSection}
          />
        )}
        {section === "workspaces" && (
          selWs == null ? (
            <WorkspacesList
              workspaces={workspaces}
              scanningId={scanningId}
              onOpen={setSelWs}
              onScan={scanWs}
            />
          ) : (
            <WorkspaceDetail
              stats={workspaces.find((w) => w.workspace.id === selWs)!}
              projects={wsProjects}
              scanning={scanningId === selWs}
              onBack={() => setSelWs(null)}
              onRescan={() => scanWs(selWs)}
              onOpenProject={setPanel}
            />
          )
        )}
        {section === "cleanup" && <Cleanup projects={allProjects} onOpenProject={setPanel} />}
        {section === "backup" && <Backup workspaces={workspaces} />}
        {section === "settings" && <Settings workspaces={workspaces} onChanged={load} />}
      </main>

      {searchOpen && (
        <SearchPalette
          wsName={wsName}
          onClose={() => setSearchOpen(false)}
          onOpen={(p) => { setPanel(p); setSearchOpen(false); }}
        />
      )}

      {panel && (
        <ProjectPanel
          project={panel}
          workspaceName={panel.workspace_id != null ? wsName.get(panel.workspace_id) : undefined}
          onClose={() => setPanel(null)}
          onToggleIgnore={toggleIgnore}
        />
      )}
    </div>
  );
}

/* ----------------------------------------------------------------- Overview */

function Icon({ name }: { name: string }) {
  const c = {
    width: 17, height: 17, viewBox: "0 0 24 24", fill: "none",
    stroke: "currentColor", strokeWidth: 1.7,
    strokeLinecap: "round" as const, strokeLinejoin: "round" as const,
  };
  switch (name) {
    case "grid": return (<svg {...c}><rect x="3" y="3" width="7" height="7" rx="1.5" /><rect x="14" y="3" width="7" height="7" rx="1.5" /><rect x="3" y="14" width="7" height="7" rx="1.5" /><rect x="14" y="14" width="7" height="7" rx="1.5" /></svg>);
    case "folder": return (<svg {...c}><path d="M3 7a2 2 0 0 1 2-2h3.5l2 2H19a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /></svg>);
    case "clean": return (<svg {...c}><path d="M12 3l1.6 4.4L18 9l-4.4 1.6L12 15l-1.6-4.4L6 9l4.4-1.6z" /><path d="M18 15l.7 2L21 18l-2.3.8L18 21l-.7-2.2L15 18l2.3-1z" /></svg>);
    case "download": return (<svg {...c}><path d="M12 4v11" /><path d="M8 11l4 4 4-4" /><path d="M5 20h14" /></svg>);
    case "sliders": return (<svg {...c}><path d="M4 7h16M4 12h16M4 17h16" /><circle cx="15" cy="7" r="2.2" fill="var(--bg-side)" /><circle cx="9" cy="12" r="2.2" fill="var(--bg-side)" /><circle cx="16" cy="17" r="2.2" fill="var(--bg-side)" /></svg>);
    case "search": return (<svg {...c}><circle cx="11" cy="11" r="7" /><path d="M21 21l-4.3-4.3" /></svg>);
    default: return null;
  }
}

function PRow({ p, onOpen }: { p: Project; onOpen: (p: Project) => void }) {
  return (
    <div className={`prow${p.ignored ? " ignored" : ""}`} onClick={() => onOpen(p)}>
      <div className="prow-main">
        <div className="prow-name">
          {p.name}
          {p.stack[0] && <span className="tag">{p.stack[0]}</span>}
          {!p.git_present && <span className="tag warn">no git</span>}
        </div>
        <div className="prow-path mono">{p.path}</div>
      </div>
      <div className="prow-size mono">{formatBytes(p.size_bytes)}</div>
      <div className="prow-junk mono">{p.junk_bytes > 0 ? formatBytes(p.junk_bytes) : "—"}</div>
      <Score value={p.health_score} size={26} />
    </div>
  );
}

function Overview({
  workspaces, projects, onOpenProject, onGo,
}: {
  workspaces: WorkspaceStats[];
  projects: Project[];
  onOpenProject: (p: Project) => void;
  onGo: (s: Section) => void;
}) {
  const [sortBy, setSortBy] = useState<"junk" | "size" | "health" | "recent">("junk");
  const [filter, setFilter] = useState<"all" | "junk" | "nogit" | "stale">("all");
  const active = projects.filter((p) => !p.ignored);
  const recoverable = workspaces.reduce((a, w) => a + w.total_junk_bytes, 0);
  const health = weightedHealth(workspaces);

  const safe = useMemo(() => {
    let t = 0;
    for (const p of active) for (const j of p.junk_detail) if (cleanupInfo(j.name).risk === "low") t += j.bytes;
    return t;
  }, [active]);

  const list = useMemo(() => {
    const l = active.filter((p) => {
      if (filter === "junk") return p.size_bytes > 0 && p.junk_bytes / p.size_bytes >= 0.5;
      if (filter === "nogit") return !p.git_present;
      if (filter === "stale") return activityBucket(p.last_activity) === "archived";
      return true;
    });
    return l.sort((a, b) => {
      if (sortBy === "size") return b.size_bytes - a.size_bytes;
      if (sortBy === "health") return a.health_score - b.health_score;
      if (sortBy === "recent") return (b.last_activity || 0) - (a.last_activity || 0);
      return b.junk_bytes - a.junk_bytes;
    });
  }, [active, filter, sortBy]);

  if (workspaces.length === 0) {
    return <Empty title="Welcome" sub="Add a workspace folder, or scan your computer to find your projects." action={<button className="btn" onClick={() => onGo("settings")}>Get started</button>} />;
  }

  return (
    <div className="page">
      <h1 className="page-h1">Overview</h1>
      <div className="statline">
        <span><b>{active.length}</b> projects</span>
        <span><b className="cleanable">{formatBytes(recoverable)}</b> recoverable</span>
        <span><b style={{ color: healthColor(health) }}>{health}</b> avg health</span>
        {safe > 0 && <button className="statline-link" onClick={() => onGo("cleanup")}>{formatBytes(safe)} safe to clean →</button>}
      </div>

      <div className="toolbar">
        <div className="segmented">
          {(["junk", "size", "health", "recent"] as const).map((k) => (
            <button key={k} className={sortBy === k ? "seg active" : "seg"} onClick={() => setSortBy(k)}>{k[0].toUpperCase() + k.slice(1)}</button>
          ))}
        </div>
        <div className="quick">
          {([["all", "All"], ["junk", "High junk"], ["nogit", "No git"], ["stale", "Stale"]] as const).map(([k, l]) => (
            <button key={k} className={filter === k ? "qf active" : "qf"} onClick={() => setFilter(k)}>{l}</button>
          ))}
        </div>
      </div>

      <div className="prows">
        <div className="prow head"><span>Project</span><span>Size</span><span>Junk</span><span>Health</span></div>
        {list.map((p) => <PRow key={p.path} p={p} onOpen={onOpenProject} />)}
        {list.length === 0 && <div className="pad muted">No projects match.</div>}
      </div>
    </div>
  );
}

/* --------------------------------------------------------------- Workspaces */

function WorkspacesList({
  workspaces, scanningId, onOpen, onScan,
}: {
  workspaces: WorkspaceStats[];
  scanningId: number | null;
  onOpen: (id: number) => void;
  onScan: (id: number) => void;
}) {
  if (workspaces.length === 0) {
    return <Empty title="No workspaces yet" sub="Add one in Settings, or find them automatically by scanning your computer." />;
  }
  return (
    <div className="page">
      <h1 className="page-h1">Workspaces</h1>
      <div className="rows">
        {workspaces.map((w) => (
          <div className="row clickable" key={w.workspace.id} onClick={() => onOpen(w.workspace.id)}>
            <span className="dot" style={{ background: w.workspace.color }} />
            <div className="row-main">
              <div className="row-title">{w.workspace.name}</div>
              <div className="row-sub">{w.workspace.path}</div>
            </div>
            <div className="row-cols">
              <Col label="projects" value={String(w.project_count)} />
              <Col label="recoverable" value={formatBytes(w.total_junk_bytes)} cls="cleanable" />
              <Col label="scanned" value={w.workspace.last_scanned ? formatActivity(w.workspace.last_scanned) : "never"} />
            </div>
            <Score value={w.health_score} />
            <button
              className="btn-soft"
              onClick={(e) => { e.stopPropagation(); onScan(w.workspace.id); }}
              disabled={scanningId === w.workspace.id}
            >
              {scanningId === w.workspace.id ? "…" : "Scan"}
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}

function WorkspaceDetail({
  stats, projects, scanning, onBack, onRescan, onOpenProject,
}: {
  stats: WorkspaceStats;
  projects: Project[];
  scanning: boolean;
  onBack: () => void;
  onRescan: () => void;
  onOpenProject: (p: Project) => void;
}) {
  const [sortBy, setSortBy] = useState<"junk" | "size" | "health" | "recent">("junk");
  const sorted = useMemo(() => {
    return [...projects].sort((a, b) => {
      if (sortBy === "size") return b.size_bytes - a.size_bytes;
      if (sortBy === "health") return a.health_score - b.health_score;
      if (sortBy === "recent") return (b.last_activity || 0) - (a.last_activity || 0);
      return b.junk_bytes - a.junk_bytes;
    });
  }, [projects, sortBy]);

  return (
    <div className="page">
      <button className="back" onClick={onBack}>← Workspaces</button>
      <div className="detail-head">
        <div>
          <h1 className="page-h1" style={{ margin: 0 }}>{stats.workspace.name}</h1>
          <div className="statline sub">
            <span><b>{stats.project_count}</b> projects</span>
            <span><b className="cleanable">{formatBytes(stats.total_junk_bytes)}</b> recoverable</span>
            <span><b style={{ color: healthColor(stats.health_score) }}>{stats.health_score}</b> health</span>
            <span className="mono dim">{stats.workspace.path}</span>
          </div>
        </div>
        <div className="head-actions">
          <button className="btn-soft" onClick={() => openFolder(stats.workspace.path)}>Open</button>
          <button className="btn" onClick={onRescan} disabled={scanning}>{scanning ? "Scanning…" : "Rescan"}</button>
        </div>
      </div>
      <div className="toolbar">
        <div className="segmented">
          {(["junk", "size", "health", "recent"] as const).map((k) => (
            <button key={k} className={sortBy === k ? "seg active" : "seg"} onClick={() => setSortBy(k)}>{k[0].toUpperCase() + k.slice(1)}</button>
          ))}
        </div>
      </div>
      <div className="prows">
        <div className="prow head"><span>Project</span><span>Size</span><span>Junk</span><span>Health</span></div>
        {sorted.map((p) => <PRow key={p.path} p={p} onOpen={onOpenProject} />)}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ Cleanup */

function Cleanup({ projects, onOpenProject }: { projects: Project[]; onOpenProject: (p: Project) => void }) {
  const active = projects.filter((p) => !p.ignored);
  const { groups, totals } = useMemo(() => {
    const agg = new Map<string, number>();
    for (const p of active) for (const j of p.junk_detail) agg.set(j.name, (agg.get(j.name) ?? 0) + j.bytes);
    const groups: Record<Risk, { name: string; bytes: number; info: CleanupInfo }[]> = { low: [], medium: [], high: [] };
    const totals: Record<Risk, number> = { low: 0, medium: 0, high: 0 };
    for (const [name, bytes] of agg) {
      const info = cleanupInfo(name);
      groups[info.risk].push({ name, bytes, info });
      totals[info.risk] += bytes;
    }
    for (const k of Object.keys(groups) as Risk[]) groups[k].sort((a, b) => b.bytes - a.bytes);
    return { groups, totals };
  }, [active]);

  const inactive = active.filter((p) => activityBucket(p.last_activity) === "archived").length;
  const noGit = active.filter((p) => !p.git_present).length;

  return (
    <div className="page">
      <h1 className="page-h1">Cleanup</h1>
      <div className="statline">
        <span><b style={{ color: "var(--good)" }}>{formatBytes(totals.low)}</b> safe</span>
        <span><b style={{ color: "var(--warn)" }}>{formatBytes(totals.medium)}</b> review</span>
        <span><b>{formatBytes(totals.high)}</b> caution</span>
      </div>

      <CleanupGroup title="Safe to remove" sub="Regenerable build &amp; dependency artifacts" rows={groups.low} />
      <CleanupGroup title="Review first" sub="Not auto-regenerable — check before removing" rows={groups.medium} />
      {groups.high.length > 0 && <CleanupGroup title="Caution" sub="Unrecognized — inspect manually" rows={groups.high} />}

      <Section title="Recommendations">
        <div className="row"><div className="row-main"><div className="row-title">Source code, Git and databases are never suggested for cleanup.</div></div></div>
        {totals.low > 0 && <div className="row"><div className="row-main"><div className="row-title">Clean {formatBytes(totals.low)} of regenerable junk</div></div></div>}
        {inactive > 0 && <div className="row"><div className="row-main"><div className="row-title">Review {inactive} inactive project{inactive === 1 ? "" : "s"}</div></div></div>}
        {noGit > 0 && <div className="row"><div className="row-main"><div className="row-title">Initialize Git in {noGit} project{noGit === 1 ? "" : "s"}</div></div></div>}
      </Section>

      <Section title="Biggest offenders">
        {[...active].filter((p) => p.junk_bytes > 0).sort((a, b) => b.junk_bytes - a.junk_bytes).slice(0, 6).map((p) => (
          <ProjectRow key={p.path} p={p} onOpen={onOpenProject} right={<span className="cleanable">{formatBytes(p.junk_bytes)}</span>} />
        ))}
      </Section>
    </div>
  );
}

function CleanupGroup({ title, sub, rows }: { title: string; sub: string; rows: { name: string; bytes: number; info: CleanupInfo }[] }) {
  if (rows.length === 0) return null;
  return (
    <Section title={title} subtitle={sub}>
      {rows.map((r) => (
        <div className="row" key={r.name}>
          <div className="row-main">
            <div className="row-title mono">{r.name}</div>
            <div className="row-sub">{r.info.regenerable ? "regenerable" : "not regenerable"}</div>
          </div>
          <span className="pct" style={{ color: riskColor(r.info.risk) }}>{r.info.confidence}%</span>
          <span className="cleanable mono">{formatBytes(r.bytes)}</span>
        </div>
      ))}
    </Section>
  );
}

/* ------------------------------------------------------------------- Backup */

function Backup({ workspaces }: { workspaces: WorkspaceStats[] }) {
  const [busyWs, setBusyWs] = useState<number | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [readiness, setReadiness] = useState<Readiness | null>(null);
  const [packBusy, setPackBusy] = useState(false);
  const [docker, setDocker] = useState<DockerStatus | null>(null);
  const [dockerBusy, setDockerBusy] = useState(false);

  useEffect(() => {
    invoke<Readiness>("recovery_readiness").then(setReadiness).catch(() => {});
    invoke<DockerStatus>("docker_status").then(setDocker).catch(() => {});
  }, []);

  async function backupWs(id: number) {
    setMsg(null);
    const dir = await pickDir();
    if (!dir) return;
    setBusyWs(id);
    try {
      const r = await invoke<BackupResult>("backup_workspace", { id, dest: dir });
      setMsg(`Backed up ${r.projects} projects (${formatBytes(r.bytes_copied)}) → ${r.dest}`);
      await openFolder(r.dest);
    } catch (e) { setMsg(String(e)); }
    finally { setBusyWs(null); }
  }
  async function createPack() {
    setMsg(null);
    const dir = await pickDir();
    if (!dir) return;
    setPackBusy(true);
    try {
      const stamp = new Date().toISOString().slice(0, 10);
      const dest = await invoke<string>("create_recovery_pack", { dir, filename: `RNZ-Recovery-${stamp}.zip` });
      setMsg(`Recovery pack → ${dest}`);
      await openFolder(dir);
    } catch (e) { setMsg(String(e)); }
    finally { setPackBusy(false); }
  }
  async function backupVolumes() {
    setMsg(null);
    const dir = await pickDir();
    if (!dir) return;
    setDockerBusy(true);
    try {
      const r = await invoke<DockerBackupResult>("backup_docker_volumes", { dest: dir });
      setMsg(`Backed up ${r.volumes} Docker volumes (${formatBytes(r.bytes)})${r.errors.length ? ` — ${r.errors.length} failed` : ""}`);
      await openFolder(r.dest);
    } catch (e) { setMsg(String(e)); }
    finally { setDockerBusy(false); }
  }

  const cats = readiness ? groupBy(readiness.items, (i) => i.category) : [];
  const present = readiness ? readiness.items.filter((i) => i.present).length : 0;

  return (
    <div className="page">
      <h1 className="page-h1">Backup &amp; Recovery</h1>
      {msg && <div className="note">{msg}</div>}

      <Section title="Projects" subtitle="Copies source + Git history, skips regenerable junk">
        {workspaces.length === 0 && <p className="muted pad">No workspaces to back up.</p>}
        {workspaces.map((w) => (
          <div className="row" key={w.workspace.id}>
            <span className="dot" style={{ background: w.workspace.color }} />
            <div className="row-main">
              <div className="row-title">{w.workspace.name}</div>
              <div className="row-sub">~{formatBytes(backupEstimate(w))} after skipping junk · {w.project_count} projects</div>
            </div>
            <button className="btn-soft" onClick={() => backupWs(w.workspace.id)} disabled={busyWs === w.workspace.id}>
              {busyWs === w.workspace.id ? "Backing up…" : "Back up"}
            </button>
          </div>
        ))}
      </Section>

      <Section
        title="Reinstall readiness"
        subtitle={readiness ? `${present} of ${readiness.items.length} dev configs found` : "Checking…"}
        action={<button className="btn" onClick={createPack} disabled={packBusy}>{packBusy ? "Packing…" : "Create recovery pack"}</button>}
      >
        {cats.map(([cat, items]) => (
          <div className="rec-group" key={cat}>
            <div className="rec-cat">{cat}</div>
            {items.map((it) => (
              <div className="row tight" key={it.key}>
                <span className="dot" style={{ background: it.present ? "var(--good)" : "#39404e" }} />
                <div className="row-main">
                  <div className="row-title sm">{it.label}{it.essential && <span className="tag">essential</span>}{it.secret && <span className="tag warn">secret</span>}</div>
                  <div className="row-sub mono">{it.path || "not found"}</div>
                </div>
                <span className="muted mono sm">{it.present ? formatBytes(it.size_bytes) : "—"}</span>
              </div>
            ))}
          </div>
        ))}
      </Section>

      {docker && docker.available && (
        <Section
          title="Docker"
          subtitle={`${docker.running ? "running" : "stopped"} · ${docker.containers.length} containers · ${docker.volumes.length} volumes`}
          action={<button className="btn-soft" onClick={backupVolumes} disabled={dockerBusy || !docker.running || docker.volumes.length === 0}>{dockerBusy ? "Backing up…" : "Back up volumes"}</button>}
        >
          <div className="chips pad">
            {docker.volumes.map((v) => <span key={v} className="chip mono">{v}</span>)}
            {docker.volumes.length === 0 && <span className="muted">No named volumes.</span>}
          </div>
        </Section>
      )}
    </div>
  );
}

/* ----------------------------------------------------------------- Settings */

function Settings({ workspaces, onChanged }: { workspaces: WorkspaceStats[]; onChanged: () => void }) {
  const [name, setName] = useState("");
  const [path, setPath] = useState("");
  const [color, setColor] = useState(COLORS[0]);
  const [discovered, setDiscovered] = useState<Project[] | null>(null);
  const [scanning, setScanning] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  async function pick() {
    const d = await pickDir();
    if (d) {
      setPath(d);
      if (!name) setName(d.split(/[\\/]/).filter(Boolean).pop() || "");
    }
  }
  async function add(p?: string, n?: string) {
    setErr(null);
    const np = p ?? path, nn = n ?? name;
    if (!np || !nn) return;
    try { await invoke("add_workspace", { name: nn, path: np, color }); setName(""); setPath(""); await onChanged(); }
    catch (e) { setErr(String(e)); }
  }
  async function remove(id: number) {
    try { await invoke("delete_workspace", { id }); await onChanged(); } catch (e) { setErr(String(e)); }
  }
  async function discover(root: string) {
    setErr(null); setScanning(true); setDiscovered(null);
    try { setDiscovered(await invoke<Project[]>("discover_projects", { root })); }
    catch (e) { setErr(String(e)); }
    finally { setScanning(false); }
  }
  const groups = useMemo(() => {
    if (!discovered) return [];
    const m = new Map<string, Project[]>();
    for (const p of discovered) { const par = parentOf(p.path); (m.get(par) ?? m.set(par, []).get(par)!).push(p); }
    return [...m.entries()].map(([par, ps]) => ({ par, ps })).sort((a, b) => b.ps.length - a.ps.length);
  }, [discovered]);

  return (
    <div className="page">
      <h1 className="page-h1">Settings</h1>
      {err && <div className="note err">{err}</div>}

      <Section title="Add a workspace">
        <div className="form">
          <div className="path-pick">
            <code className="code">{path || "no folder selected"}</code>
            <button className="btn-soft" onClick={pick}>Browse…</button>
          </div>
          <input className="input" placeholder="Name" value={name} onChange={(e) => setName(e.target.value)} />
          <div className="swatches">
            {COLORS.map((c) => (
              <button key={c} className={`swatch${c === color ? " active" : ""}`} style={{ background: c }} onClick={() => setColor(c)} />
            ))}
          </div>
          <button className="btn" disabled={!name || !path} onClick={() => add()}>Add workspace</button>
        </div>
      </Section>

      <Section title="Find projects" subtitle="Scan your computer when you don't know where they live">
        <div className="head-actions pad">
          <button className="btn" onClick={() => discover("")} disabled={scanning}>{scanning ? "Scanning…" : "Scan my home folder"}</button>
          <button className="btn-soft" onClick={async () => { const d = await pickDir(); if (d) discover(d); }} disabled={scanning}>Choose folder…</button>
        </div>
        {discovered && groups.map((g) => (
          <div className="row" key={g.par}>
            <div className="row-main">
              <div className="row-title mono sm">{g.par}</div>
              <div className="row-sub">{g.ps.length} projects · {formatBytes(g.ps.reduce((a, b) => a + b.size_bytes, 0))}</div>
            </div>
            <button className="btn-soft" onClick={() => add(g.par, g.par.split(/[\\/]/).filter(Boolean).pop() || g.par)}>+ Add</button>
          </div>
        ))}
      </Section>

      <Section title="Manage workspaces">
        {workspaces.map((w) => (
          <div className="row" key={w.workspace.id}>
            <span className="dot" style={{ background: w.workspace.color }} />
            <div className="row-main"><div className="row-title">{w.workspace.name}</div><div className="row-sub mono sm">{w.workspace.path}</div></div>
            <button className="btn-soft danger" onClick={() => remove(w.workspace.id)}>Remove</button>
          </div>
        ))}
      </Section>

      <Section title="About">
        <div className="row"><div className="row-main"><div className="row-title">RNZ Workstation Manager</div><div className="row-sub">Local-first · no cloud · no telemetry. Your data never leaves this machine.</div></div></div>
      </Section>
    </div>
  );
}

/* ----------------------------------------------------------- Search palette */

function SearchPalette({
  wsName, onClose, onOpen,
}: {
  wsName: Map<number, string>;
  onClose: () => void;
  onOpen: (p: Project) => void;
}) {
  const [q, setQ] = useState("");
  const [results, setResults] = useState<Project[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => { inputRef.current?.focus(); }, []);
  useEffect(() => {
    const v = q.trim();
    if (!v) { setResults([]); return; }
    let cancelled = false;
    invoke<Project[]>("search_projects", { query: v }).then((r) => { if (!cancelled) setResults(r); }).catch(() => {});
    return () => { cancelled = true; };
  }, [q]);

  return (
    <div className="palette-overlay" onClick={onClose}>
      <div className="palette" onClick={(e) => e.stopPropagation()}>
        <input
          ref={inputRef}
          className="palette-input"
          placeholder="Search projects…"
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
        <div className="palette-results">
          {q.trim() && results.length === 0 && <div className="palette-empty">No matches</div>}
          {results.map((p) => (
            <button className="palette-row" key={p.path} onClick={() => onOpen(p)}>
              <span className="palette-name">{p.name}</span>
              {p.workspace_id != null && wsName.get(p.workspace_id) && (
                <span className="tag">{wsName.get(p.workspace_id)}</span>
              )}
              <span className="palette-path mono">{p.path}</span>
              <span className="palette-size mono">{formatBytes(p.size_bytes)}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------- Project panel */

function ProjectPanel({
  project: p, workspaceName, onClose, onToggleIgnore,
}: {
  project: Project;
  workspaceName?: string;
  onClose: () => void;
  onToggleIgnore: (p: Project) => void;
}) {
  const [copied, setCopied] = useState(false);
  const factors = healthFactors(p);
  const breakdown = p.junk_detail.filter((b) => b.bytes > 0);
  const maxJunk = Math.max(1, ...breakdown.map((b) => b.bytes));

  async function copyPath() {
    try { await navigator.clipboard.writeText(p.path); setCopied(true); setTimeout(() => setCopied(false), 1400); } catch { /* */ }
  }
  async function backup() {
    const dir = await pickDir();
    if (!dir) return;
    try { const r = await invoke<BackupResult>("backup_project", { path: p.path, name: p.name, dest: dir }); await openFolder(r.dest); } catch (e) { console.error(e); }
  }

  return (
    <>
      <div className="panel-scrim" onClick={onClose} />
      <aside className="panel">
        <div className="panel-head">
          <div>
            <div className="panel-title">{p.name}</div>
            {workspaceName && <div className="panel-ws">{workspaceName}</div>}
          </div>
          <button className="panel-x" onClick={onClose}>✕</button>
        </div>

        <div className="panel-actions">
          <button onClick={() => openFolder(p.path)}>Open</button>
          <button onClick={() => openVSCode(p.path)}>VS Code</button>
          <button onClick={copyPath}>{copied ? "Copied" : "Copy path"}</button>
          <button onClick={backup}>Back up</button>
        </div>

        <code className="panel-path mono">{p.path}</code>

        <div className="panel-grid">
          <Meta label="Size" value={formatBytes(p.size_bytes)} />
          <Meta label="Recoverable" value={formatBytes(p.junk_bytes)} cls="cleanable" />
          <Meta label="Confidence" value={`${p.confidence}%`} color={confidenceColor(p.confidence)} />
          <Meta label="Health" value={String(p.health_score)} color={healthColor(p.health_score)} />
          <Meta label="Git" value={p.git_present ? "yes" : "no"} />
          <Meta label="Last activity" value={formatActivity(p.last_activity)} />
        </div>

        {p.stack.length > 0 && (
          <div className="panel-chips">{p.stack.map((s) => <span key={s} className="chip">{s}</span>)}</div>
        )}

        <div className="panel-sec">Why this health</div>
        <ul className="factors">
          {factors.map((f, i) => (
            <li key={i} className={f.good ? "good" : "bad"}>
              <span>{f.good ? "+" : "−"} {f.label}</span>
              {f.delta !== 0 && <span className="mono">{f.delta > 0 ? `+${f.delta}` : f.delta}</span>}
            </li>
          ))}
        </ul>

        {breakdown.length > 0 && (
          <>
            <div className="panel-sec">Junk breakdown</div>
            <div className="bars">
              {breakdown.map((b: JunkEntry) => (
                <div className="bar-row" key={b.name}>
                  <span className="bar-label mono">{b.name}</span>
                  <div className="bar-track"><div className="bar-fill" style={{ width: `${(b.bytes / maxJunk) * 100}%` }} /></div>
                  <span className="bar-size mono">{formatBytes(b.bytes)}</span>
                </div>
              ))}
            </div>
          </>
        )}

        <button className={`ignore${p.ignored ? " on" : ""}`} onClick={() => onToggleIgnore(p)}>
          {p.ignored ? "Restore — this is a project" : "Ignore — not a project"}
        </button>
      </aside>
    </>
  );
}

/* -------------------------------------------------------------- UI atoms */

function Section({ title, subtitle, onMore, action, children }: {
  title: string; subtitle?: string; onMore?: () => void; action?: ReactNode; children: ReactNode;
}) {
  return (
    <section className="block">
      <div className="block-head">
        <div>
          <div className="block-title">{title}</div>
          {subtitle && <div className="block-sub">{subtitle}</div>}
        </div>
        {action}
        {onMore && <button className="more" onClick={onMore}>View all →</button>}
      </div>
      <div className="rows">{children}</div>
    </section>
  );
}

function ProjectRow({ p, onOpen, right }: { p: Project; onOpen: (p: Project) => void; right?: ReactNode }) {
  return (
    <div className={`row clickable${p.ignored ? " ignored" : ""}`} onClick={() => onOpen(p)}>
      <div className="row-main">
        <div className="row-title">
          {p.name}
          {p.stack[0] && <span className="tag">{p.stack[0]}</span>}
          {!p.git_present && <span className="tag warn">no git</span>}
        </div>
        <div className="row-sub mono">{p.path}</div>
      </div>
      <div className="row-right">{right}</div>
    </div>
  );
}

function Col({ label, value, cls }: { label: string; value: string; cls?: string }) {
  return (
    <div className="col">
      <div className={`col-val ${cls ?? ""}`}>{value}</div>
      <div className="col-label">{label}</div>
    </div>
  );
}

function Meta({ label, value, cls, color }: { label: string; value: string; cls?: string; color?: string }) {
  return (
    <div className="meta">
      <div className="meta-label">{label}</div>
      <div className={`meta-val ${cls ?? ""}`} style={color ? { color } : undefined}>{value}</div>
    </div>
  );
}

function Empty({ title, sub, action }: { title: string; sub: string; action?: ReactNode }) {
  return (
    <div className="empty-state">
      <div className="empty-title">{title}</div>
      <div className="empty-sub">{sub}</div>
      {action}
    </div>
  );
}

function groupBy<T>(arr: T[], key: (t: T) => string): [string, T[]][] {
  const m = new Map<string, T[]>();
  for (const it of arr) { const k = key(it); (m.get(k) ?? m.set(k, []).get(k)!).push(it); }
  return [...m.entries()];
}
