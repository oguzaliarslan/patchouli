# Taiga → Rust + Tauri Migration Plan

Source analysed: <https://github.com/erengy/taiga> (cloned 2026-11-17).

---

## 1. What Taiga is

A Windows desktop app that **detects anime videos playing in local media players or browsers**, works out which anime and episode they are, and **updates the user's list on AniList, Kitsu or MyAnimeList**. It also keeps a local library and history, can search and browse anime, and shares "now watching" through Discord Rich Presence and a webhook.

### Branches and which one to port

| Branch | State | Stack | Use for the port |
|---|---|---|---|
| `master` (v1.4.1, Mar 2024) | Released, maintenance only | Win32/C++17, custom UI toolkit, XML files | Reference for features v2 doesn't have yet (torrents/RSS, seasons, stats) |
| `develop` (v2.0.0, active, last commit Nov 2026) | Rewrite in progress | **C++23 + Qt6** (Widgets, Network, Sql), SQLite | **Primary source.** It's cleaner and modular, and maps almost 1:1 onto a Rust workspace |

`develop` also has `src/v1/` (about 34.6k LOC). It's the old code kept for reference and isn't compiled. The live v2 code is about 41k LOC, and roughly half of that is Qt GUI.

---

## 2. Architecture of v2 (`develop/src`)

```
main.cpp ─► taiga::Application (QApplication)
              ├─ single-instance (QSharedMemory + QLocalServer)
              ├─ settings / accounts / session (QSettings INI files)
              ├─ anime::db, anime::history  (SQLite)
              ├─ sync::queue                (SQLite-backed offline update queue)
              ├─ track::media::detection    (polls players via Anisthesia, every 3s)
              ├─ track::updateSession       (countdown → commit state machine)
              ├─ share::discord::presence, share::http::webhook
              └─ gui::MainWindow (Home, Search, List, History, Library, Torrents*, Profile*)
```

### Modules

| Module | Files | Responsibility | Notes for the port |
|---|---|---|---|
| `base/` | chrono, file, log, settings, string, xml, rss | Utilities, `FuzzyDate`, INI settings wrapper | Replaced by std plus crates (`chrono`/`time`, `tracing`, `serde`) |
| `media/` | anime, anime_list, anime_db, anime_history, anime_season, anime_list_export, anime_utils | Domain model and SQLite persistence | Port the domain types straight across. Keep the **SQL schema** (`resources/sql/*.sql`) |
| `sync/` | service, queue, search_params, `anilist/`, `kitsu/`, `myanimelist/` | Service abstraction, three API clients, offline queue with retry and reconcile | Becomes an async `trait SyncService`. Queue logic: dirty-field bitflags, retry count, `pending_delete` |
| `track/` | episode, media, media_player, recognition*, scanner, play, update_* | Detection polling, filename parsing (Anitomy), title normalisation, title→ID cache, episode redirection (anime-relations), library-folder scan, update decision and session | **The core value of the app.** Port it carefully, test-first |
| `share/` | discord, discord_ipc, discord_rpc, event, http | Discord IPC over a local named pipe; JSON webhook (`version:1` schema) | Small and self-contained |
| `compat/` | anime, list, history, settings, common | Import v1 XML data (db, list, history, queue, settings) | Needed only if users migrate from Taiga v1. Use `quick-xml` |
| `taiga/` | application, settings, accounts, session, network, path, version, orange | App bootstrap, typed settings keys, credentials, proxy, portable data path | `orange` is a background thread (Easter egg, can be skipped) |
| `gui/` | main, common, models, settings, media, search, list, history, library, utils | Qt Widgets UI: list/table/card views, poster loading, tray, theming, 9 settings pages | **Rewritten** as a web frontend |
| `resources/` | sql, gql (8 AniList queries), icons (SVG), styles, translations | Static assets | `.gql` and `.sql` files copy over verbatim. Icons are reusable. Translations need conversion |

### External dependencies (git submodules)

| Dep | License | Role | Rust strategy |
|---|---|---|---|
| **anitomy** | MPL-2.0 | Anime filename parser (header-only C++, about 1.5k LOC). Has `test/data.json` | **Port to a Rust crate** and use `data.json` as the parity test suite. Existing Rust crates (`anitomy-rs`) port the old v1 algorithm, so check them against `data.json` before adopting one |
| **anisthesia** | MIT | Media-player detection (Windows: window enumeration, open-file handles via `NtQuerySystemInformation`, UI Automation for browser tabs, Matroska title reading) + `players.anisthesia` data file (56 players) | **Port to Rust** using the `windows` crate. Reuse the data file as-is (public domain) |
| **anime-relations** | Public domain | Episode redirection rules (`MAL\|Kitsu\|AniList:range -> …`) | Bundle the `.txt` and write a parser (about 100 LOC). Support remote refresh |
| nstd, semaver | MIT | Small std helpers, semver | `semver` crate |
| utf8proc | MIT | Unicode normalisation | `unicode-normalization`, `caseless`, `deunicode` |

### Data model (SQLite, from `resources/sql`)

`anime`, `anime_list` (with `pending_delete`), `anime_settings` (per-anime display title, folder, synonyms), `history` (anime_id, episode, time), `queue` (anime_id, dirty bitflags, time, retry_count, last_error), `meta`.

Settings, accounts and session state are **INI files** through `QSettings`. Keys are listed in `taiga/settings.hpp` (for example `track.update.delay`, `sync.service`, `sharing.webhook.url`).

### Detection → update flow (the critical path)

```
poll (3s) ─► anisthesia: enumerate windows → match player → strategy
             (window_title regex | open_files | ui_automation)
          ─► file/title string
          ─► anitomy::parse → Episode{title, episode range, season, ext…}
             (fall back to parent directory names for the title/season)
          ─► normalize(title)  (unicode, punctuation, roman/ordinal/season numbers, transliteration)
          ─► cache lookup  normalized title → {anime_id: weight}
          ─► validate episode type/number; anime-relations redirect if out of range
          ─► UpdateDecision {Allow | Confirm | Deny} + Reason
          ─► UpdateSession phases: Idle → Countdown/WaitingForClose → Confirming → Committed/Cancelled
             (pauses when the player loses focus, trigger = AfterDelay | OnPlayerClose)
          ─► write list entry + history → sync::queue.push(id, dirty) → service API
          ─► share: Discord presence + webhook events (playing/stopped/updated)
```

### Auth flows today

| Service | Flow | Port consideration |
|---|---|---|
| AniList | Implicit grant. The user opens the browser and pastes the token back | Register **our own** client. Use the Tauri deep-link plugin (`tracker://auth/anilist`) so nobody has to paste |
| MyAnimeList | OAuth2 auth code + PKCE. Redirect to `taiga.moe/api/myanimelist/auth`, then paste the code | Taiga's redirect URL and client ID **cannot be reused**. Register our own app with a loopback (`http://127.0.0.1:<port>`) or deep-link redirect |
| Kitsu | Password grant with a public client id and secret, plus refresh token | Same flow is OK. Store tokens in the **OS keychain** (`keyring` crate), not INI |

---

## 3. Key decisions and constraints

1. **License: Taiga is GPL-3.0.** A port that translates its code is a derivative work, so **our app must be GPL-3.0** too. The alternative is a clean-room reimplementation from behaviour and specs, which is slower and riskier. Anitomy (MPL-2.0) and Anisthesia (MIT) are permissive. **This has to be decided before any code is written.**
2. **Hardcoded client IDs and secrets** in Taiga belong to the Taiga project. We register our own apps with AniList, MAL and Kitsu.
3. **Platform scope.** Taiga is Windows-only (detection uses Win32 APIs). Tauri runs cross-platform, so we put detection behind a trait and **ship Windows first**. macOS and Linux get a stub detector at first, which later can use MPRIS on Linux and AppleScript/Accessibility on macOS.
4. **Port v2 (`develop`) semantics, not v1.** Features that exist only in v1 (torrents/RSS feeds, seasons browser, stats, mIRC/Twitter sharing, auto-updater) go in a later phase.
5. **Keep the SQLite schema compatible** so a v2 Taiga database can be imported directly.

---

## 4. Target architecture

### Stack

- **Shell:** Tauri 2.x (Rust backend, WebView2 on Windows)
- **Backend:** Rust 2024 edition, `tokio`, `reqwest` (rustls, proxy support), `rusqlite` (bundled) + `refinery`/hand-rolled migrations, `serde`, `tracing`, `thiserror`/`anyhow`, `keyring`, `windows` crate, `quick-xml`, `regex`, `bitflags`, `chrono`
- **Typed IPC:** `tauri-specta` to generate TypeScript bindings for commands and events
- **Tauri plugins:** single-instance, tray (core), deep-link, notification, autostart, opener, window-state, log, (later) updater
- **Frontend (recommended):** React + TypeScript + Vite, TanStack Query (server state) + TanStack Table/Virtual (large lists), Zustand (UI state), Tailwind or CSS modules, i18next (translations)

### Cargo workspace layout

```
tracker-app/
├─ Cargo.toml                  # workspace
├─ crates/
│  ├─ core/                    # domain: Anime, ListEntry, Status, Type, FuzzyDate, Fields bitflags, Season
│  ├─ db/                      # rusqlite pool, migrations (from resources/sql), repos: anime, list, history, settings, queue
│  ├─ anitomy/                 # Rust port of anitomy + data.json parity tests
│  ├─ detect/                  # trait MediaDetector; windows impl (anisthesia port), players.anisthesia parser
│  ├─ recognition/             # normalize, title cache, relations, validate, identify, path parsing, scanner
│  ├─ track/                   # UpdateDecision + UpdateSession state machine (pure, clock-injected)
│  ├─ sync/                    # trait SyncService; anilist (GraphQL), myanimelist (REST v2), kitsu (JSON:API); queue
│  ├─ share/                   # discord IPC client (named pipe / unix socket), webhook
│  └─ compat/                  # Taiga v1 XML + Taiga v2 SQLite/INI importers
├─ src-tauri/                  # app crate: state wiring, commands, events, tray, deep links, settings store
│   └─ resources/              # players.anisthesia, anime-relations.txt, gql/*.gql, icons
└─ ui/                         # React/TS frontend
```

Each crate apart from `detect` (Windows parts) and `src-tauri` should be **pure and unit-testable without Tauri**.

### Runtime model

- `AppState` (in `tauri::State`) holds `Db`, `Settings` (`RwLock` + `watch` channel for change notifications), `SyncManager`, `Detector`, `UpdateSession`, `ShareHub`.
- Background `tokio` tasks:
  - **detector loop** (interval from settings, 3s by default) → `mpsc` → **update session task** (1s tick)
  - **sync queue worker** (processes on push, plus periodic retry with backoff)
  - **share task** (subscribes to episode and update-state broadcasts)
- Backend → UI: Tauri events such as `now-playing`, `update-state`, `queue-changed`, `list-changed`, `sync-progress`, `error`.
- UI → backend: commands such as `list_get`, `list_update_entry`, `anime_get`, `search`, `history_get`, `settings_get/set`, `auth_start/complete`, `sync_now`, `update_accept/cancel`, `library_scan`, `play_episode`, `import_taiga`.
- Images: fetch and cache posters on disk in the Rust backend, served through a custom `asset:`/URI protocol (this replaces `gui/utils/image_provider`).

### C++ → Rust mapping

| C++ (Qt) | Rust |
|---|---|
| `QObject` signals/slots | `tokio::sync::{broadcast, watch, mpsc}` + Tauri events |
| `QTimer` | `tokio::time::interval` |
| `QRestAccessManager` / `QNetworkRequestFactory` | `reqwest::Client` with middleware for auth, retry and token refresh |
| `QSqlDatabase` / `QSqlQuery` | `rusqlite` + `r2d2` / `spawn_blocking` |
| `QSettings` INI | `serde` TOML/JSON settings file + `keyring` for secrets |
| `Q_DECLARE_FLAGS(Fields)` | `bitflags!` |
| `QSharedMemory` + `QLocalServer` single instance | `tauri-plugin-single-instance` |
| `QSystemTrayIcon` | Tauri tray API |
| `QLocalSocket` (Discord) | `tokio::net::windows::named_pipe` / `UnixStream` |
| `QXmlStreamReader` (compat) | `quick-xml` |
| Qt Linguist `.ts` | Convert to i18next JSON |

---

## 5. Phased plan

Estimates assume one experienced Rust developer. With a dedicated frontend developer, phases 3 and 6 can run in parallel with phases 2, 4 and 5.

### Phase 0: Decisions and setup (about 1 week)
- [ ] Decide on licensing (GPL-3.0 port vs clean-room)
- [ ] Pick a product name and app identifier. Register OAuth apps on AniList, MAL and Kitsu
- [ ] Scaffold: `cargo` workspace, `create-tauri-app` (React-TS), CI (GitHub Actions: fmt, clippy, test, `tauri build` on windows-latest)
- [ ] Vendor resources: `players.anisthesia`, `anime-relations.txt`, `gql/*.gql`, SVG icons
- **Exit:** an empty Tauri window builds in CI and produces an installer artifact

### Phase 1: Core domain and storage (1–2 weeks) ✅
- [x] `core`: port `anime.hpp`, `anime_list.hpp`, `anime_settings.hpp`, `anime_season`, `FuzzyDate`, all enums with stable integer values (they're stored in the DB)
- [x] `db`: migrations from `create*.sql`, repositories for anime, list, settings, history, queue, meta. Fix the `anime_list` FK, which points at a non-existent `media` table
  - Migration 2 also re-keys `anime_list` by `media_id`: Taiga keys it by the service entry ID, which is 0 for every unsynced entry, so they overwrote each other
  - Foreign keys stay unenforced, as in Taiga (history and queue can refer to deleted anime)
- [x] Settings store with the same key namespace as `taiga/settings.hpp`, plus change notifications (`crates/settings`, Taiga's `settings.json` format, unknown keys preserved)
- [x] Data path: portable mode (a `portable` file next to the exe) vs `%APPDATA%`
- **Exit:** repository round-trip tests pass, and a sample DB loads (`crates/db/tests/taiga_v2.rs` builds a database the way Taiga writes it)

### Phase 2: Sync services (3–4 weeks), AniList and MyAnimeList ✅
- [x] Service client (`tracker_sync::Client`, an enum rather than a trait object since the methods are async): user, fetch_anime, search, fetch_list, save/delete entry, anime_page_url
- [x] **AniList**: GraphQL with the existing `.gql` files, error mapping, duplicate entries from custom lists skipped. Rating-system display (`anilist_ratings`) is left to the phase 3 UI; scores are stored 0–100 and the account's score format is saved
- [x] **MyAnimeList**: REST v2, PKCE, refresh-on-401 (refreshed tokens saved), paged list fetch with pruning
- [ ] **Kitsu**: deferred
- [x] **Queue**: port of `sync/queue.cpp`, including dirty-field merge, `modifiedWhileProcessing`, reconcile with the remote entry, retry count and last error, 10 s debounce and 5 min retry. Fetching skips entries with unsent changes (Taiga could overwrite them)
- [x] Auth via a loopback server (`http://localhost:47312/callback/<service>`). Tokens in Windows Credential Manager, split into chunks because an entry holds at most 1280 characters
- [x] The local list belongs to one service (`meta.service`); logging in to the other asks before replacing it
- [ ] Proxy support (`network.proxy.*`)
- **Exit:** integration tests against `wiremock` pass. Manual login and sync need registered client IDs (see README)

### Phase 3: UI foundation (3–4 weeks, can run in parallel) ✅
- [x] App shell: sidebar navigation (Home, Search, List, History, Library, Settings), status bar with sync state and errors, now-playing placeholder on Home
- [x] Anime list: virtualised table **and** card view, text and type filters, sorting (click for primary, Shift+click for secondary, persisted), status tabs with counts, context menu (`media_menu`: details, edit, watched episode, status, score, open page, copy, remove)
- [x] Anime detail dialog (`media_dialog`): details, list-entry editor (fuzzy dates, score in the account's format), per-anime settings (display title, folder, alternative titles)
- [x] Search page with filters (season, year, type, status); season browsing
- [x] History page, grouped by day
- [x] Settings: Application, Accounts, Library, Recognition, Media players (from `players.anisthesia`), Streaming, Sharing, Advanced
- [x] Theming: light/dark/system (page and title bar), NSFW hiding, title language (romaji/english/native)
- [x] Tray icon (show, sync, quit), close/start to tray, window position and size persistence
- [x] Browser preview with sample data (`src/mocks.ts`) for UI work without the app
- [ ] Posters load straight from the services' URLs; a disk cache is still to do
- [ ] i18n (phase 6)
- **Exit:** a user can log in, see the synced list, edit entries and search, with no detection yet (verified against sample data; a real login needs client IDs)

### Phase 4: Recognition (2–3 weeks, test-driven) ✅
- [x] `anitomy` crate: port parser and options. It matches the C++ library element for element (kind, value, byte position) on all 275 golden cases: Anitomy's `test/data.json` inputs plus extra edge cases, with output recorded from the C++ build (`crates/anitomy/tests/oracle`). C++ quirks are kept for parity and commented
- [x] `recognition`: `normalize` (unicode NFKC, punctuation, ordinals, roman numerals, season numbers, transliteration), title index (titles + synonyms + user synonyms, with weights), `parse_parent_directories`, validate, `identify` with an explanation of each candidate
  - Deviation from Taiga v2: with a season above 1, `Title Season N` is looked up before the bare title, since Anitomy separates the season (Taiga v2 matches `Title S2 - 05` to season 1)
- [x] `relations`: parse `anime-relations.txt` (`~`, `?`, `!`, per-service column), lookup by service id; a copy in the data folder overrides the bundled file
- [x] Library scanner: `find_episode`, `find_folder`, `is_inside_library_folders` (`walkdir`), library scan, plus play next / random / specific episode and open folder
- [x] Golden corpus from the C++ build (for Anitomy; recognition is covered by unit tests, since the C++ recognizer needs Taiga's whole app)
- **Exit:** recognition tests pass, and a filename can be debugged from Settings › Recognition › Test a file name

### Phase 5: Media detection and auto-update (3–4 weeks, highest technical risk) ✅
- [x] `players.anisthesia` parser (tab-indented format, regexes start with `^`), same state machine and errors as Anisthesia
- [x] Windows detector (`crates/detect`, Anisthesia port with the `windows` crate):
  - window enumeration (`EnumWindows`, class name, process exe, the same filters)
  - `window_title` strategy (title formats as full-match regexes, first non-empty group)
  - `open_files` strategy (`NtQuerySystemInformation(SystemExtendedHandleInformation)` + `DuplicateHandle` + `GetFinalPathNameByHandle`), skipping handles that aren't read-only like Anisthesia does, so pipes that hang aren't touched. Detection runs on its own thread, so a slow poll never blocks the UI
  - `ui_automation` strategy for browsers (`IUIAutomation`, tree walk skipping documents)
  - Matroska title reading: not ported; Taiga v2 doesn't use it
- [x] `track`: `update_decision.cpp` and the `UpdateSession` state machine as **pure code driven by ticks**, with tests for focus pause, OnPlayerClose, closing too soon, dismissal, already-watched, skips-ahead, late identification
- [x] Commit path: list entry via `entry_with_episode_watched` → synchronizer (history + queue + events); OS notification for the Confirm phase
- [x] Disabled players, detection interval, streaming (browser) switch, `libraryOnly`, `pauseWhenUnfocused`
- [x] Verified on this machine: real mpv (open files + title) and Firefox (UI Automation) detected; end to end in the app with a fake MPC-HC (`crates/detect/examples/fake_player.rs`): episode 3 recorded after the 10 s delay, history added, change queued
- [ ] Manual check with VLC, PotPlayer and Chrome/Edge (needs those players and a logged-in account)
- **Exit:** watching a file updates the list end to end (done with the fake player; real players still to be tried by hand)

### Phase 6: Sharing and polish (1–2 weeks) ✅
- [x] Discord IPC client (`crates/share`): handshake, framing, ping/pong, reconnect with backoff (2 s doubling to 1 min), stop on invalid application ID, `SET_ACTIVITY` (port of `discord_rpc::toJson` with text trimming and up to 2 buttons). Tested against a fake Discord on a named pipe
  - Needs our own Discord application ID (`TRACKER_DISCORD_APP_ID`); Taiga's would show "Taiga" on profiles
- [x] Webhook: Taiga's v1 event schema (playing/stopped/updated, custom headers), 1.5 s debounce, private entries never shared. Verified end to end against a local listener
- [x] List export (`anime_list_export`): Markdown and MyAnimeList XML
- [ ] i18n: **not needed yet**. Taiga's `resources/translations` holds only English plural forms, so there's nothing to convert. Revisit when there are translators
- [x] Logging to `<data>/logs/tracker_<date>.log`, plus `--debug`/`--verbose` CLI flags

### Phase 7: Data migration from Taiga (1 week) ✅
- [x] Import from a Taiga **v2** data folder (`crates/compat/src/v2.rs`): `media.sqlite` opened read-only and not migrated, `settings.json` merged key by key (unknown keys and wrong types skipped), username from `accounts.json`. Taiga v2 uses JSON, not INI
- [x] Import from a Taiga **v1** data folder (`crates/compat/src/v1.rs`, port of `compat/*`): invalid character references removed, the extra `<meta>` root handled, anime with the active service's `<id name="…">`, list, history (local times), the unsent queue applied to entries with the matching dirty fields (`mode="delete"` → pending delete), `settings.xml` mapped to v2 keys, per-anime folders and alternative titles
- [x] Login tokens are never imported (they belong to Taiga's application); the user logs in again
- [x] First-run prompt on Home when Taiga data is found in the default locations and the local list is empty ("Import" / "Not now", remembered in `meta.taiga_import`), plus Settings › Advanced › Import from Taiga with a folder picker (portable Taiga folders too). Importing over a local list asks first; the import is one savepoint

### Phase 8: Release (1–2 weeks)
- [ ] Code signing (Windows Authenticode), NSIS/MSI installer + portable zip
- [ ] `tauri-plugin-updater` with signed update manifests
- [ ] Remote refresh of `anime-relations.txt` and `players.anisthesia` (Taiga has this in v1 `taiga/update.cpp`)
- [ ] Crash/panic reporting (local log, opt-in upload)
- [ ] Beta with power users, compare recognition accuracy against Taiga

### Phase 9 (later): v1 feature parity and cross-platform
- Torrents/RSS feed aggregator + filters (v1 `track/feed*`, about 1.9k LOC)
- Seasonal anime browser, stats page, Profile page
- Linux detection (MPRIS over D-Bus via `zbus`) and macOS detection (AppleScript/Accessibility)

**Rough total for a Windows MVP (phases 0–8): about 4–6 months for one developer, or about 3 months for two.**

---

## 6. Testing strategy

- **Parity tests against the C++ build.** Build Taiga `develop` once and use it as an oracle: dump recognition results and update decisions for a corpus of filenames and states, then assert the Rust output matches.
- **Unit tests:** anitomy (`data.json`), normalize, relations parser, update decision table, queue merge/reconcile, rating conversions, Discord/webhook JSON.
- **HTTP fixtures:** `wiremock` + recorded responses per service (with tokens scrubbed).
- **Windows detection:** manual test matrix per player and browser, plus a small fake-player test exe that sets window titles and opens files, for CI.
- **Frontend:** Vitest for components, Playwright/WebDriver (`tauri-driver`) smoke tests for main flows.

---

## 7. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| GPL-3.0 obligations | Legal | Decide in phase 0. Keep attribution and license headers if porting |
| `open_files` handle enumeration hangs or needs privileges | Detection breaks for some players | Worker thread with timeout, skip non-file handle types, fall back to window title |
| UI Automation from Rust is verbose and fragile | Streaming detection gaps | Isolate it in its own module and thread, port anisthesia's tree-walk logic exactly, defer if needed |
| Recognition accuracy regressions | Wrong episodes get updated | Oracle parity tests, keep the Confirm phase on by default during beta |
| API changes and rate limits (MAL, Kitsu, AniList) | Sync failures | Centralised retry/backoff, surfacing `last_error` per queue item in the UI |
| Large lists (thousands of entries) in a WebView | UI jank | Virtualised lists, paging/filtering in Rust, send only diffs through events |
| OAuth redirect handling in a desktop app | Login friction | Deep links + loopback fallback, manual-paste fallback as last resort |

---

## 8. Immediate next steps

1. Confirm the license approach and the frontend framework (React+TS recommended above).
2. Register OAuth apps on AniList, MAL and Kitsu.
3. Scaffold the workspace and CI (phase 0), then start phase 1 (`core` + `db`) and phase 4's `anitomy` port in parallel, since both are pure Rust and easy to test.
