# Setup

## Logging in to AniList and MyAnimeList

If your build has no client ID for your list service, register an app with that service.

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

Tokens are kept in Windows Credential Manager (under `app.tracker.desktop`), not in the data folder. AniList and MyAnimeList use different anime IDs, so the local list belongs to one service at a time (the main list); making the other one the main list asks before replacing it.

To keep both lists, use **Copy changes here** on the other service under *Settings › Accounts*. Every change to the main list is then also sent to the other service, with the whole entry as it is locally. Anime are matched using the MyAnimeList IDs that AniList stores. Anime with no match there are listed with an error and are left out. **Copy whole list now** sends every entry once, for example right after you turn copying on. Changes made directly on the other service's website are not copied back.

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

## Other platforms

Player detection on Linux uses MPRIS (mpv needs the `mpv-mpris` script), and on macOS asks VLC, QuickTime Player, Safari and Chromium browsers through AppleScript. Both are compile-checked from Windows but not yet tested on those systems.

## Data

Data (`media.sqlite`, `settings.json`, `accounts.json`) goes in `%APPDATA%\app.tracker.desktop`. To run in portable mode, put an empty file named `portable` next to the executable, and data goes in `data\` beside it instead.

Once a day, Patchouli copies the database, settings, accounts file and torrent archive to a dated folder in `backups\` inside the data folder, and keeps the last 7 (*Settings › Advanced › Backups*). Restoring one backs up the current data first, then restarts Patchouli with the backup in place.

## Marking episodes watched by playback position

mpv, MPC-HC, MPC-BE and VLC can report how far into the episode you are. With **Settings › Recognition › Use the playback position** on (the default), an episode in one of these players counts as watched once playback reaches the set percentage (85% by default), however long that took. Other players, and these when Patchouli can't reach them, use the timer.

Each player needs its interface turned on:

- **mpv / mpv.net**: add `input-ipc-server=mpvsocket` to `mpv.conf`, or start it with `--input-ipc-server=mpvsocket`.
- **MPC-HC / MPC-BE**: *Options › Player › Web Interface › Listen on port* (13579 by default).
- **VLC**: *Tools › Preferences › Show settings: All › Interface › Main interfaces*, tick *Web*, then set a password under *Main interfaces › Lua* and enter it in Patchouli. The port is 8080 unless you've changed it.
