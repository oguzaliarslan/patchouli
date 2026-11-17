# Tracker

A Rust + Tauri port of [Taiga](https://github.com/erengy/taiga): detects anime playing in media players and syncs progress to AniList, Kitsu or MyAnimeList. See [MIGRATION_PLAN.md](MIGRATION_PLAN.md) for the plan.

"Tracker" and `app.tracker.desktop` are placeholder names.

## Layout

```
crates/
  core/         domain types (Anime, ListEntry, FuzzyDate, Season, ServiceId, ...)
  db/           SQLite storage: migrations and per-table repositories (opens Taiga v2 databases)
  settings/     settings.json store (Taiga v2 format) with change notifications
  anitomy/      filename parser (phase 4)
  detect/       media player detection (phase 5)
  recognition/  title matching, anime-relations, library scan (phase 4)
  track/        update decision and session state machine (phase 5)
  sync/         AniList / MAL / Kitsu clients and update queue (phase 2)
  share/        Discord presence and webhook (phase 6)
  compat/       Taiga data importers (phase 7)
src-tauri/      Tauri app: wiring, commands, resources
ui/             React + TypeScript + Vite frontend
```

## Prerequisites (Windows)

- Rust stable (`rustup default stable`)
- Node.js LTS
- Visual Studio Build Tools with "Desktop development with C++"
- WebView2 (preinstalled on Windows 11)

## Commands

```bash
npm install
```

```bash
npm run dev
```

```bash
cargo test --workspace
```

```bash
npm run build
```

`npm run dev` starts Vite on port 1420 and opens the Tauri window.

Data (`media.sqlite`, `settings.json`) goes in `%APPDATA%\app.tracker.desktop`. To run in portable mode, put an empty file named `portable` next to the executable, and data goes in `data\` beside it instead. `npm run build` produces an NSIS installer in `target/release/bundle/nsis/`.
