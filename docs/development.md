# Development

## Prerequisites (Windows)

- Rust stable (`rustup default stable`)
- Node.js LTS
- Visual Studio Build Tools with "Desktop development with C++"
- WebView2 (preinstalled on Windows 11)

## Run from source

```powershell
npm ci
npm run dev
```

Run `cargo test --workspace` to test the Rust workspace. Run `npm run build` to build an NSIS installer under `target/release/bundle/nsis/`. See [Windows releases](releases.md) for MSI, portable packages, and signing.

`npm run dev` starts Vite on port 1420 and opens the Tauri window.

To see what player detection finds right now, or to test it without a real player:

```bash
cargo run -p patchouli-detect --example detect
```

```bash
cargo build -p patchouli-detect --example fake_player
```

`fake_player` opens a file and shows a window with a given class and title; copy it to a player's executable name (e.g. `mpc-hc64.exe`) and run it with `<window class> <title> <file> <seconds>`.

To work on the UI without the app, run `npm run ui:dev` and open <http://localhost:1420> in a browser. Outside the app window the UI uses made-up sample data (`ui/src/mocks.ts`) instead of the backend.

## Layout

```
crates/
  core/         domain types (Anime, ListEntry, FuzzyDate, Season, ServiceId, ...)
  db/           SQLite storage: migrations and per-table repositories (opens Taiga v2 databases)
  settings/     settings.json store (Taiga v2 format) with change notifications
  anitomy/      filename parser, a port of Anitomy checked against the C++ library
  detect/       media player detection (Windows: a port of Anisthesia; Linux: MPRIS; macOS: AppleScript)
  recognition/  title normalisation and matching, anime-relations, library scanning
  track/        what's playing and when to update the list
  sync/         AniList and MyAnimeList clients, browser login, update queue
  share/        Discord Rich Presence and the sharing webhook
  compat/       Taiga v1/v2 data importers
  feed/         torrent RSS feeds and filters (Taiga v1's Torrents)
src-tauri/      Tauri app: wiring, commands, resources
ui/             React + TypeScript + Vite frontend
```

## Logs

Logs go in `logs\patchouli_<date>.log` in the data folder: warnings and errors normally, more with `--debug` or `--verbose` (debug builds log debug messages too).

## Beta diagnostics

For a recognition discrepancy, test the same filename or path in Patchouli and Taiga with equivalent list data and library settings. In **Settings ? Recognition**, select **Copy recognition report** after running a test. Record the anime ID and episode range reported by both apps, and note any differences in service, library settings or relation rules. Review the copied report before sharing it.

The latest Rust panic report is written to `logs/panic.log` in the data folder. Review it before sharing it with a bug report.

The opt-in credential test writes to the OS credential store. Run it with `cargo test -p patchouli -- --ignored`.
