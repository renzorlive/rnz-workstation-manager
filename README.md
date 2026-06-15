<div align="center">

# 🗂️ RNZ Workstation Manager

### Your `C:\` drive is a crime scene. This is the detective.

You have **300 projects** scattered across three drives, `node_modules` folders
older than your career, and a `Downloads\public_html\public_html\` you're too
scared to open. You don't know what's there, what's junk, or whether you could
rebuild your machine if the SSD died tonight.

**Same.** That's why this exists.

[![Built for Windows](https://img.shields.io/badge/Built_for-Windows_devs-0078D6?logo=windows&logoColor=white)](#)
[![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white)](https://v2.tauri.app)
[![Rust](https://img.shields.io/badge/Rust-blazingly_honest-DEA584?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-22c55e.svg)](LICENSE)
[![Cloud](https://img.shields.io/badge/Cloud-absolutely_not-f8716a)](#)

**No cloud. No login. No telemetry. No "upgrade to Pro." Your data dies on your machine, where it belongs.**

</div>

---

## 🪟 Yes, it's a Windows thing

Built by a Windows dev, for Windows devs. It speaks `%APPDATA%`, `%LOCALAPPDATA%`,
PowerShell profiles, that cursed versioned `Packages\Microsoft.WindowsTerminal_*`
folder, and the seventeen places your tools hide their configs. macOS and Linux
users: it'll mostly work and we won't stop you, but the love letters are addressed
to `C:\Users\you`.

## 🤨 The questions this answers

> *"How much of my 500 GB is actually code and how much is `node_modules`?"*

Spoiler: it's mostly `node_modules`. It's always `node_modules`.

> *"Which of these 280 folders are real projects and which are random extracted zips?"*

It knows. It even gives each one a **confidence score**, like a bouncer.

> *"If Windows Update nukes my drive tonight, am I cooked?"*

There's a **Reinstall Readiness** score for that. Find out how cooked you are, then un-cook yourself.

## ✨ What it actually does

### 🔍 Finds your stuff
Smart project detection (`.git`, `package.json`, `Cargo.toml`, `go.mod`, `*.sln`, …)
with stack sniffing. It's not dumb about it either:
- **Ignores vendored deps** — your WordPress plugin's `vendor_prefixed/polyfill-php80` is **not** a project, and it knows that now.
- **WordPress = one project**, not 47 plugins pretending to be your work.
- **Monorepos** (`frontend` + `backend` + `bot`) collapse into one entry instead of three.
- **"Find projects" mode** scans your whole machine for when you genuinely have no idea where anything is. (We've all been there. It's fine.)

### 🧮 Understands your stuff
- A **health score** (0–100) that actually explains itself — no git? no readme? 4 GB of junk? It'll tell you why your project is sad.
- **Detailed junk breakdown** — `node_modules`, `.next`, `dist`, `target`, `coverage`, logs, archives — measured separately so you know exactly where the bytes went.
- **History & deltas** — watch your junk grow over time like a Tamagotchi you're neglecting.
- **Sort by junk, size, health.** Find the worst offenders in one click.

### 🧹 Cleans your stuff (carefully)
The **Cleanup Analyzer** sorts every junk category into **Safe / Review / Caution**
with a confidence %, and shows how many GB you'd reclaim — **without deleting anything.**
No "oops" button. Source code, Git history and databases are never, ever touched.

### 💾 Backs up the stuff that matters
- **Dev-aware backup** — copies your source **and full Git history**, but skips the regenerable `node_modules`/`dist`/`target` bloat. Back up the 240 GB that matters, not the 420 GB of garbage.
- **Reinstall Readiness + Recovery Pack** — one click zips up SSH keys, git config, and configs for **VS Code, Cursor, Windsurf, Trae, Zed, Neovim**, the AI gang (**Claude, Codex, Gemini, opencode, Aider, Continue, Manus**), PowerShell, Windows Terminal, Docker, AWS/gcloud/Azure, and more. Reinstall Windows, unzip, you're back in an hour.
- **Docker volume backup** — exports your named volumes (the actual database data) to tarballs, because `docker compose up` won't bring back the data you lost.

### ⌨️ Feels like a real app
Sidebar nav, a **`Cmd/Ctrl+K` command palette** that searches every project instantly,
and a native side panel. Linear/Raycast energy, zero admin-panel energy.

## 🚀 Run it (Windows)

You'll need: **Node 18+**, **Rust** ([rustup](https://rustup.rs)), and
[MS C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
(WebView2 already ships with Windows 11).

```powershell
npm install
npm run tauri dev
```

Build the installer:
```powershell
npm run tauri build
# → src-tauri\target\release\bundle\
```

First Rust build takes a minute. Go make coffee. ☕

## 🔒 The privacy bit (it's short)

Nothing leaves your machine. No accounts, no analytics, no phoning home.
The only things that ever move are the backup folder and recovery zip **you**
create — and yes, that zip contains your **SSH keys**, so guard it like one.

## 🏗️ Under the hood

**Rust** (Tauri) does the heavy lifting — filesystem scanning, detection, junk
math, backups, zipping. **React + TypeScript** for a UI that doesn't feel like a
web page in a trench coat. **SQLite**, local, no server. That's it.

## 🗺️ Roadmap (aka "later")

- [ ] One-click cleanup (once we trust the model not to nuke anything)
- [ ] Scheduled scans
- [ ] Trend charts (once there's enough history to be interesting)
- [ ] Export inventory to CSV / Markdown

## 🤝 Contributing

PRs welcome — see [CONTRIBUTING.md](CONTRIBUTING.md). Found a folder it
mis-detected? Open an issue with the folder layout; the detector lives to be
tuned.

## 📄 License

[MIT](LICENSE) © RNZ — do whatever, just don't blame us if you `rm -rf` the wrong thing.
