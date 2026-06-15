<div align="center">

# 🗂️ RNZ Workstation Manager

### Find, understand, clean, back up, and recover your entire dev workstation — locally.

A **local-first** desktop app that scans your disks, builds a live inventory of every
software project, tells you what's junk and what's recoverable, and packages
everything you need to rebuild your machine after a disk failure.

[![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white)](https://v2.tauri.app)
[![Rust](https://img.shields.io/badge/Rust-stable-DEA584?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![React](https://img.shields.io/badge/React-18-61DAFB?logo=react&logoColor=black)](https://react.dev)
[![SQLite](https://img.shields.io/badge/SQLite-local-003B57?logo=sqlite&logoColor=white)](https://www.sqlite.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-22c55e.svg)](LICENSE)

**No cloud · No telemetry · No account · Your data never leaves your machine.**

</div>

---

## Why

If you've ever had 100+ projects scattered across drives and asked yourself:

- *"What do I actually have on disk, and how much space is it eating?"*
- *"What can I safely delete to free up 50 GB right now?"*
- *"If this SSD died tomorrow, could I rebuild my workstation in an hour?"*

…this is built to answer exactly those questions — for developers, without the noise.

## ✨ Features

### 🔍 Discover & inventory
- **Smart project detection** by markers (`.git`, `package.json`, `Cargo.toml`, `composer.json`, `go.mod`, `pyproject.toml`, `*.sln`, …) with stack inference (Next.js, React, Rust, Go, PHP, .NET, Python).
- **Dependency-aware** — never mistakes vendored deps for projects (`node_modules`, `vendor`, `vendor_prefixed`, `.pnpm`, …), recognizes **WordPress** installs as one project, and groups **monorepos** (frontend + backend + bot) into a single entry.
- **Confidence score** per project + a "likely dependency" flag, and one-click **Ignore** to hide false positives (persists across rescans).
- **Find projects** mode — scan your whole computer when you don't know where things live, grouped by location.
- **Global search**, **sort** (size / junk / health / activity) and **quick filters** (active, inactive, no-git, high-junk, low-health).

### 🧮 Understand
- **Health score** (0–100) with a fully explainable breakdown (git, README, junk ratio, activity).
- **Detailed junk breakdown** per project and workspace — `node_modules`, `.next`, `dist`, `build`, `coverage`, `target`, archives, logs — each measured separately.
- **Historical scans & trends** — every scan is snapshotted; cards show deltas vs. the last scan; a History tab tracks projects / health / junk over time.
- **Top offenders** — biggest recoverable space and worst-health projects at a glance.

### 🧹 Clean (analysis-first, no accidental deletes)
- **Cleanup Analyzer** classifies every junk category as **Safe / Review / Caution** with a cleanup-confidence %, a dry-run recovery plan, and an explicit "never touched" guarantee for source/git/databases.

### 💾 Back up & recover
- **Dev-aware backup** — copies project source **including full `.git` history** but **skips** regenerable dependency/build dirs. Back up the GBs that matter, not the bloat. Non-destructive.
- **Reinstall Readiness** — a categorized checklist of ~40 dev artifacts (SSH/GPG keys, git config, **AI tools**: Claude, Codex, Gemini, opencode, Aider, Continue, Cursor, Windsurf, Manus, Trae; editors; shells & terminals; cloud CLIs; package managers) with a readiness score over the essentials.
- **Recovery Pack** — one click bundles every detected config (+ a project inventory + a Docker manifest) into a single dated zip.
- **Docker volume backup** — exports named volumes (your databases/data) to tarballs, plus a manifest of containers/images/volumes/networks.

## 🖼️ Screenshots

> _Add screenshots/GIFs here — `docs/screenshot-overview.png`, etc._

## 🚀 Getting started

### Prerequisites
- **Node.js** 18+
- **Rust** (stable) — install via [rustup](https://rustup.rs)
- Tauri OS prerequisites — see the [Tauri guide](https://v2.tauri.app/start/prerequisites/) (Windows: MS C++ Build Tools + WebView2, which ships with Win11)

### Run in dev
```bash
npm install
npm run tauri dev
```

### Build a release binary
```bash
npm run tauri build
```
Installers land in `src-tauri/target/release/bundle/`.

## 🏗️ How it works

```
React + TypeScript (UI)  ──invoke──>  Rust commands (Tauri)
                                          │
              scanner · detector · junk · health · backup · recovery · docker
                                          │
                                    SQLite (local)
```

- **Rust** does the heavy lifting: parallel-friendly filesystem walking, project/stack detection, junk classification, health scoring, backup copying, and zip packaging.
- **SQLite** (bundled, local) stores workspaces, projects, per-category junk, and scan snapshots in the OS app-data dir.
- **React + Vite** renders a fast, native-feeling desktop UI inside a Tauri WebView.

See [`src-tauri/src/`](src-tauri/src) for the backend modules and [`src/`](src) for the UI.

## 🔒 Privacy

Everything runs and stays on your machine. There is no network calls, no analytics,
and no account. The only data that ever leaves is what **you** explicitly export
(a backup folder or a recovery zip you create). The Recovery Pack can include
**secrets** (SSH keys, cloud credentials) by design — treat that zip like a credential.

## 🗺️ Roadmap

- [ ] One-click cleanup (after the analyzer model is trusted)
- [ ] Scheduled / automatic scans + filesystem watching
- [ ] Export inventory (CSV / Markdown)
- [ ] Trend charts once enough history accrues
- [ ] Backup profiles with size estimation

## 🤝 Contributing

Contributions welcome! See [CONTRIBUTING.md](CONTRIBUTING.md). Open an issue to
discuss bigger changes first.

## 📄 License

[MIT](LICENSE) © RNZ
