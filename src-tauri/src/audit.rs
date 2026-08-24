//! Pre-reinstall audit aggregator (§16-18). Read-only: it enriches the already
//! scanned projects with Git + secrets state, adds a machine software
//! inventory, derives severity/critical-actions, and renders a Markdown report.

use std::path::Path;

use crate::model::{AuditReport, EnvFile, ItemType, Project, ProjectAudit};
use crate::{git, secrets, software};

/// Ordered severity so a project keeps only its worst issue level.
fn worst(a: &'static str, b: &'static str) -> &'static str {
    let rank = |s: &str| match s {
        "critical" => 2,
        "warning" => 1,
        _ => 0,
    };
    if rank(b) > rank(a) {
        b
    } else {
        a
    }
}

/// Run the audit over the given (already scanned) projects.
pub fn run(projects: Vec<Project>, now: i64) -> AuditReport {
    let mut rows: Vec<ProjectAudit> = Vec::new();
    let (mut git_repos, mut not_git, mut dirty, mut no_remote, mut unpushed) = (0, 0, 0, 0, 0);
    let (mut env_total, mut tracked_secrets) = (0usize, 0usize);

    // ---- Discovery buckets (over every non-ignored discovered item) ----
    let (mut discovered, mut real_projects, mut containers) = (0usize, 0usize, 0usize);
    let (mut caches, mut application_data, mut other_items, mut unknowns) = (0, 0, 0, 0);
    for p in &projects {
        if p.ignored {
            continue;
        }
        discovered += 1;
        match p.item_type {
            ItemType::Project => real_projects += 1,
            ItemType::ProjectContainer => containers += 1,
            ItemType::Cache | ItemType::DependencyStore => caches += 1,
            ItemType::ApplicationData => application_data += 1,
            ItemType::Unknown => {
                unknowns += 1;
                other_items += 1;
            }
            _ => other_items += 1,
        }
    }

    // Git/secrets audit runs ONLY on real projects (§9).
    for p in &projects {
        if p.ignored || !p.item_type.is_project() {
            continue;
        }
        let dir = Path::new(&p.path);
        if !dir.is_dir() {
            continue;
        }
        let g = git::audit(dir);
        let envs: Vec<EnvFile> = secrets::scan(dir);
        env_total += envs.len();

        let mut issues: Vec<String> = Vec::new();
        let mut sev = "ok";

        if g.is_repo {
            git_repos += 1;
            if g.dirty {
                dirty += 1;
                issues.push(format!("{} uncommitted change(s)", g.modified));
                sev = worst(sev, "critical");
            }
            if g.untracked > 0 {
                issues.push(format!("{} untracked file(s)", g.untracked));
                sev = worst(sev, "warning");
            }
            if !g.has_remote {
                no_remote += 1;
                issues.push("no git remote configured".into());
                sev = worst(sev, "critical");
            }
            if g.ahead > 0 {
                unpushed += 1;
                issues.push(format!("{} unpushed commit(s)", g.ahead));
                sev = worst(sev, "warning");
            } else if g.has_remote && !g.has_upstream {
                unpushed += 1;
                issues.push("branch has no upstream".into());
                sev = worst(sev, "warning");
            }
            if g.detached {
                issues.push("detached HEAD".into());
                sev = worst(sev, "warning");
            }
        } else {
            not_git += 1;
            issues.push("not a git repository".into());
            sev = worst(sev, "critical");
        }

        for e in &envs {
            if e.tracked_by_git {
                tracked_secrets += 1;
                issues.push(format!("secret file tracked by git: {}", e.name));
                sev = worst(sev, "critical");
            }
        }

        rows.push(ProjectAudit {
            name: p.name.clone(),
            path: p.path.clone(),
            stack: p.stack.clone(),
            size_bytes: p.size_bytes,
            git: g,
            env_files: envs,
            severity: sev.to_string(),
            issues,
        });
    }

    // Worst first for the report.
    rows.sort_by_key(|r| match r.severity.as_str() {
        "critical" => 0,
        "warning" => 1,
        _ => 2,
    });

    let mut critical: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    if dirty > 0 {
        critical.push(format!(
            "{dirty} project(s) have uncommitted changes — commit before reinstall"
        ));
    }
    if no_remote > 0 {
        critical.push(format!(
            "{no_remote} git project(s) have no remote — push or back up before reinstall"
        ));
    }
    if not_git > 0 {
        critical.push(format!(
            "{not_git} project(s) are not under Git — back up or `git init` before reinstall"
        ));
    }
    if tracked_secrets > 0 {
        critical.push(format!(
            "{tracked_secrets} secret file(s) tracked by Git — rotate and remove from history"
        ));
    }
    if unpushed > 0 {
        warnings.push(format!(
            "{unpushed} project(s) have unpushed or upstream-less commits — verify the remote has them"
        ));
    }
    warnings.push("Git remote reachability not checked (offline audit) — verify remotes manually.".into());

    let mut discovery_warnings: Vec<String> = Vec::new();
    if unknowns > 0 {
        discovery_warnings.push(format!(
            "{unknowns} unclassified director(ies) surfaced — review whether any are real projects"
        ));
    }

    let software = software::inventory();
    let safe_to_reinstall = critical.is_empty();

    AuditReport {
        generated_at: now,
        discovered_items: discovered,
        real_projects,
        containers,
        caches,
        application_data,
        other_items,
        discovery_warnings,
        total_projects: rows.len(),
        git_repos,
        not_git,
        dirty,
        no_remote,
        unpushed,
        env_files_total: env_total,
        tracked_secrets,
        projects: rows,
        software,
        critical,
        warnings,
        safe_to_reinstall,
    }
}

/// Render a human-readable Markdown report.
pub fn to_markdown(r: &AuditReport) -> String {
    let mut s = String::new();
    s.push_str("# RNZ Workstation — Pre-Reinstall Audit\n\n");
    s.push_str(&format!(
        "**Safe to reinstall:** {}\n\n",
        if r.safe_to_reinstall { "YES" } else { "NO" }
    ));

    s.push_str("## Discovery\n\n");
    s.push_str(&format!("- Discovered items: {}\n", r.discovered_items));
    s.push_str(&format!("- Real projects: {}\n", r.real_projects));
    s.push_str(&format!("- Project containers: {}\n", r.containers));
    s.push_str(&format!("- Cache / dependency stores: {}\n", r.caches));
    s.push_str(&format!("- Application data: {}\n", r.application_data));
    s.push_str(&format!("- Other (files/archives/unknown): {}\n\n", r.other_items));

    s.push_str("## Summary (real projects)\n\n");
    s.push_str(&format!("- Projects audited: {}\n", r.total_projects));
    s.push_str(&format!("- Git repos: {} (not git: {})\n", r.git_repos, r.not_git));
    s.push_str(&format!("- Uncommitted (dirty): {}\n", r.dirty));
    s.push_str(&format!("- No remote: {}\n", r.no_remote));
    s.push_str(&format!("- Unpushed / no upstream: {}\n", r.unpushed));
    s.push_str(&format!(
        "- Env files: {} (tracked by git: {})\n\n",
        r.env_files_total, r.tracked_secrets
    ));

    if !r.critical.is_empty() {
        s.push_str("## 🔴 Critical actions\n\n");
        for (i, c) in r.critical.iter().enumerate() {
            s.push_str(&format!("{}. {}\n", i + 1, c));
        }
        s.push('\n');
    }
    if !r.warnings.is_empty() {
        s.push_str("## 🟡 Warnings\n\n");
        for w in &r.warnings {
            s.push_str(&format!("- {}\n", w));
        }
        s.push('\n');
    }

    s.push_str("## Projects\n\n");
    for p in &r.projects {
        let mark = match p.severity.as_str() {
            "critical" => "🔴",
            "warning" => "🟡",
            _ => "🟢",
        };
        s.push_str(&format!("### {mark} {}\n", p.name));
        s.push_str(&format!("- Path: `{}`\n", p.path));
        if !p.stack.is_empty() {
            s.push_str(&format!("- Stack: {}\n", p.stack.join(", ")));
        }
        if p.git.is_repo {
            s.push_str(&format!(
                "- Git: {} @ {} · remote: {}\n",
                p.git.branch,
                if p.git.head.is_empty() { "(no commits)" } else { &p.git.head },
                if p.git.has_remote { p.git.remotes.join(", ") } else { "none".into() }
            ));
        }
        if p.issues.is_empty() {
            s.push_str("- Issues: none\n");
        } else {
            for i in &p.issues {
                s.push_str(&format!("- ⚠ {}\n", i));
            }
        }
        s.push('\n');
    }

    s.push_str("## Installed software\n\n");
    for t in &r.software {
        s.push_str(&format!(
            "- {} — {}\n",
            t.name,
            if t.found { t.version.as_str() } else { "not found" }
        ));
    }
    s
}
