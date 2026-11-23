# Development

## What you need (Windows)

- Rust stable (`rustup default stable`)
- Node.js LTS
- Visual Studio Build Tools with "Desktop development with C++"
- WebView2, which Windows 11 already has

## Running it

```powershell
npm ci
npm run dev
```

That starts Vite on port 1420 and opens the app window. To log in from a dev build, set the client IDs first (see [Setup](setup.md#logging-in)).

`npm run build` makes an installer in `target\release\bundle\nsis\`. For the MSI, the portable ZIP and signing, see [Releases](releases.md). `cargo test --workspace` runs the Rust tests.

## Working on the UI without the app

`npm run ui:dev`, then open <http://localhost:1420> in a browser. Outside the app window, the UI runs on made-up sample data from `ui/src/mocks.ts`. Every page works, and nothing touches your real list.

You can open a page with some view settings already chosen, which is how the README screenshots were taken. For example: `http://localhost:1420/?page=seasons&seasons.groupBy="airing"`. Each parameter except `page` is a saved view setting, and its value is JSON.

## Testing detection

To see what player detection finds right now:

```bash
cargo run -p patchouli-detect --example detect
```

To test without a real player, build the fake one:

```bash
cargo build -p patchouli-detect --example fake_player
```

Copy it to a player's file name (for example `mpc-hc64.exe`) and run it with `<window class> <title> <file> <seconds>`. It opens the file and shows a window with that class and title.

## Where things are

```
crates/
  core/         anime, list entries, dates, seasons
  db/           SQLite storage (opens Taiga 2 databases as they are)
  settings/     settings.json, in Taiga 2's format
  anitomy/      file name parser, a port of Anitomy checked against the C++ original
  detect/       player detection (Windows: a port of Anisthesia; Linux: MPRIS; macOS: AppleScript)
  recognition/  title matching, anime-relations, library scanning
  track/        deciding when an episode counts as watched
  sync/         AniList and MyAnimeList, logins, the queue of unsent changes
  share/        Discord and the webhook
  compat/       Taiga 1.x and 2 importers
  feed/         torrent feeds and filters
src-tauri/      the app itself: commands, tray, poster cache, bundled data
ui/             React + TypeScript frontend
```

## Logs

Logs are in the `logs` folder of the data folder, one file per day. Normally only warnings and errors are written. Start the app with `--debug` or `--verbose` for more; debug builds always write debug messages.

If the app crashes, the last Rust panic is in `logs\panic.log`. Have a look before attaching it to a bug report.

## When recognition gets it wrong

Try the same file name in Patchouli and in Taiga with the same list and library settings. In Settings › Recognition, test the file name, then click **Copy recognition report**. Note which anime and episode each app picked. Check the copied report before you share it, since it contains your file paths.

## Credential test

There's one test that writes to Windows Credential Manager, so it doesn't run by default. Run it with `cargo test -p patchouli -- --ignored`.
