# Patchouli

Patchouli detects anime playing in media players and syncs your progress to AniList or MyAnimeList. It is a Rust and Tauri port of [Taiga](https://github.com/erengy/taiga). See [MIGRATION_PLAN.md](MIGRATION_PLAN.md) for the migration plan.

Patchouli retains its existing app identifier, `app.tracker.desktop`, so existing installations can access their data and saved logins.

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

To see what player detection finds right now, or to test it without a real player:

```bash
cargo run -p patchouli-detect --example detect
```

```bash
cargo build -p patchouli-detect --example fake_player
```

`fake_player` opens a file and shows a window with a given class and title; copy it to a player's executable name (e.g. `mpc-hc64.exe`) and run it with `<window class> <title> <file> <seconds>`.

To work on the UI without the app, run `npm run ui:dev` and open <http://localhost:1420> in a browser. Outside the app window the UI uses made-up sample data (`ui/src/mocks.ts`) instead of the backend.

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
cargo test -p patchouli -- --ignored
```

## Discord status

Create an application at <https://discord.com/developers/applications> (its name is what Discord shows, e.g. "Watching Patchouli"), and set its ID:

```powershell
$env:TRACKER_DISCORD_APP_ID = "123456789012345678"
```

Under *Rich Presence › Art Assets*, upload images named `default`, `anilist` and `myanimelist`; posters from the services are used when available.

## Importing from Taiga

If Taiga's data is found (`%APPDATA%\erengy\taiga\data` for Taiga 2, `%APPDATA%\Taiga\data` for Taiga 1.x) and the list is empty, Home offers to import it. Otherwise use *Settings › Advanced › Import from Taiga*, which also accepts a portable Taiga folder. The list, history, unsent changes, per-anime settings and settings are imported; Taiga's files aren't changed. Logins aren't imported, so log in again afterwards.

## Torrents

*Torrents* checks an RSS feed (Tokyo Toshokan by default; set it in *Settings › Torrents*) or searches Nyaa, recognises each release and runs Taiga's filters: by default new episodes of anime you're watching are selected, and dropped, unlisted, watched and already-downloaded ones are discarded, preferring 1080p. *Download selected* saves the `.torrent` files (or opens magnet links) and opens them with your BitTorrent client, telling known clients to save into the anime's folder. Automatic checks are off until you turn them on.

## Seasons and profile

*Seasons* lists a season's anime from your list service. *Profile* shows your account and list statistics.

## Other platforms

Player detection on Linux uses MPRIS (mpv needs the `mpv-mpris` script), and on macOS asks VLC, QuickTime Player, Safari and Chromium browsers through AppleScript. Both are compile-checked from Windows but not yet tested on those systems.

## Logs

Logs go in `logs\patchouli_<date>.log` in the data folder: warnings and errors normally, more with `--debug` or `--verbose` (debug builds log debug messages too).

## Data

Data (`media.sqlite`, `settings.json`, `accounts.json`) goes in `%APPDATA%\app.tracker.desktop`. To run in portable mode, put an empty file named `portable` next to the executable, and data goes in `data\` beside it instead. `npm run build` produces an NSIS installer in `target/release/bundle/nsis/`.

## Windows releases

Run `powershell -ExecutionPolicy Bypass -File scripts/release.ps1` from the project root. It stages the x64 NSIS and MSI installers and a portable ZIP in a new folder under `target/releases`. Use `-NsisOnly` when building just the NSIS installer and ZIP.

Set `PATCHOULI_SIGNING_THUMBPRINT` to the thumbprint of an Authenticode certificate with its private key available in the Windows certificate store. Omit it to build unsigned Windows packages.

For an update release, set `TAURI_SIGNING_PRIVATE_KEY` to the path of the updater private key and provide its password through `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Run the release script with `-SignedUpdates` to generate installer signatures and `latest.json`. The updater requires signatures that bind each installer to the version in the manifest. Attach the generated installers, signatures, ZIP and manifest to the public GitHub release tagged `v<version>`. The script uses `oguzaliarslan/patchouli` by default. Use `-Repository owner/repo` to build for another release repository.

Installed copies can check for updates under **Settings ? Application**. Portable users can extract the new ZIP into their existing portable installation folder while the app is closed. Keep the existing `data` folder.

Automatic checks also refresh episode relations and player definitions daily from their upstream repositories. The app uses bundled data if the refresh fails and no prior cache exists. Updated player definitions apply on restart. A custom `anime-relations.txt` in the data folder takes precedence.

## Beta diagnostics

For a recognition discrepancy, test the same filename or path in Patchouli and Taiga with equivalent list data and library settings. In **Settings ? Recognition**, select **Copy recognition report** after running a test. Record the anime ID and episode range reported by both apps, and note any differences in service, library settings or relation rules. Review the copied report before sharing it.

The latest Rust panic report is written to `logs/panic.log` in the data folder. Review it before sharing it with a bug report.
