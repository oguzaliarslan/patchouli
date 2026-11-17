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
  sync/         AniList and MyAnimeList clients, browser login, update queue
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

## Logging in to AniList and MyAnimeList

Taiga's client IDs belong to Taiga, so register your own app with each service and give the app its client ID:

| Service | Register at | Redirect URL |
|---|---|---|
| AniList | <https://anilist.co/settings/developer> | `http://localhost:47312/callback/anilist` |
| MyAnimeList | <https://myanimelist.net/apiconfig> (App Type: *other*) | `http://localhost:47312/callback/myanimelist` |

Then set the client IDs as environment variables, either when running the app or when building it (to bake them in):

```powershell
$env:TRACKER_ANILIST_CLIENT_ID = "12345"
$env:TRACKER_MAL_CLIENT_ID = "abcdef0123456789"
npm run dev
```

`TRACKER_OAUTH_PORT` changes the redirect port (47312 by default); the redirect URLs registered with the services must match it. No client secrets are needed: AniList uses the implicit grant and MyAnimeList uses PKCE.

Tokens are kept in Windows Credential Manager (under `app.tracker.desktop`), not in the data folder. AniList and MyAnimeList use different anime IDs, so the local list belongs to one service at a time; logging in to the other one asks before replacing it.

To run the test that writes to the real credential store:

```bash
cargo test -p tracker-app -- --ignored
```

## Data

Data (`media.sqlite`, `settings.json`, `accounts.json`) goes in `%APPDATA%\app.tracker.desktop`. To run in portable mode, put an empty file named `portable` next to the executable, and data goes in `data\` beside it instead. `npm run build` produces an NSIS installer in `target/release/bundle/nsis/`.
