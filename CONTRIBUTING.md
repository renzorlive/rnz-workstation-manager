# Contributing

Thanks for your interest in improving RNZ Workstation Manager!

## Dev setup
```bash
npm install
npm run tauri dev
```

## Before a PR
- Frontend type-checks: `npx tsc --noEmit`
- Rust builds & lints: `cd src-tauri && cargo fmt && cargo clippy && cargo build`
- Keep changes focused; open an issue first for anything large.

## Project layout
- `src/` — React + TypeScript UI
- `src-tauri/src/` — Rust backend
  - `scanner.rs` — filesystem walk + project grouping
  - `detector.rs` — project/stack/WordPress detection
  - `junk.rs` — junk categories
  - `health.rs` — health score
  - `db.rs` — SQLite storage
  - `backup.rs` / `recovery.rs` / `docker.rs` — backup & recovery
  - `lib.rs` — Tauri commands

## Detection tuning
The detector is the heart of the product. If you hit a false positive/negative,
include the folder structure in your issue so we can extend the rules.
