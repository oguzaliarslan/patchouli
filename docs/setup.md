# Setup

## Logging in

Logging in opens your browser, where you approve Patchouli on AniList or MyAnimeList. For that, the app needs a client ID from the service. If your copy doesn't have one built in (you'll get an error when you try to log in), register an app yourself:

| Service | Register at | Redirect URL |
|---|---|---|
| AniList | <https://anilist.co/settings/developer> | `http://localhost:47312/callback/anilist` |
| MyAnimeList | <https://myanimelist.net/apiconfig> (App Type: *other*) | `http://localhost:47312/callback/myanimelist` |

Then put the IDs in environment variables. Set them before building to build them in, or before starting the app:

```powershell
$env:TRACKER_ANILIST_CLIENT_ID = "12345"
$env:TRACKER_MAL_CLIENT_ID = "abcdef0123456789"
```

You don't need a client secret for either service. If port 47312 is taken, set `TRACKER_OAUTH_PORT` and change the redirect URLs to match.

Your login is stored in Windows Credential Manager under `app.tracker.desktop`, not in the data folder.

## Using AniList and MyAnimeList together

The two services number their anime differently, so your list in Patchouli belongs to one of them at a time. That's your main list. Switching the main list to the other service asks before replacing what's there.

You can still keep both accounts up to date. Under Settings › Accounts, turn on **Copy changes here** for the other service, and every change you make from then on is sent there too. **Copy whole list now** sends everything once, which is handy right after you turn copying on. Anime are matched by the MyAnimeList IDs that AniList keeps, and the few without a match are listed so you can fix them by hand.

Changes made on the other service's website don't come back. **Compare lists** shows every anime where the two lists disagree and lets you pick which side wins.

## Discord

Patchouli can show what you're watching on your Discord profile. Create an application at <https://discord.com/developers/applications>. Its name is what Discord shows, so something like "Watching Patchouli". Then set its ID:

```powershell
$env:TRACKER_DISCORD_APP_ID = "123456789012345678"
```

Under Rich Presence › Art Assets, upload images named `default`, `anilist` and `myanimelist`. The anime's poster is used instead when there is one.

## Importing from Taiga

If Patchouli finds Taiga's data and your list is empty, Home offers to import it. Otherwise use Settings › Advanced › Import from Taiga. It looks in `%APPDATA%\erengy\taiga\data` (Taiga 2) and `%APPDATA%\Taiga\data` (Taiga 1.x), or you can point it at a portable Taiga folder.

It brings over your list, history, changes Taiga hadn't sent yet, per-anime folders and titles, and your settings. Taiga's own files aren't touched. Logins can't be imported, so log in again afterwards.

## Players

Patchouli recognises episodes in the players Taiga supports, including mpv, VLC, MPC-HC, MPC-BE and PotPlayer, and in browsers. By default an episode counts as watched after it has been playing for two minutes. You can change the delay, or have it update when you close the player, under Settings › Recognition.

mpv, MPC-HC, MPC-BE and VLC can also tell Patchouli how far into the episode you are. With that on, an episode counts once you reach 85% of it, however long that took. Each player needs its interface turned on:

- mpv or mpv.net: add `input-ipc-server=mpvsocket` to `mpv.conf`.
- MPC-HC or MPC-BE: Options › Player › Web Interface › Listen on port. Leave it on 13579.
- VLC: Tools › Preferences, show all settings, then Interface › Main interfaces and tick Web. Set a password under Main interfaces › Lua and enter the same one in Patchouli. The port is 8080 unless you changed it.

If Patchouli can't reach the player, it falls back to the timer.

## Torrents

The Torrents page reads an RSS feed (Tokyo Toshokan unless you pick another in Settings › Torrents) or searches Nyaa. It recognises each release and runs Taiga's filters over them. Out of the box that means new episodes of what you're watching get selected, preferring 1080p, while anything dropped, not in your list, already watched or already downloaded is skipped.

**Download selected** saves the `.torrent` files, or opens the magnet links, in your BitTorrent client. For clients Patchouli knows, it also tells them to save into the anime's folder. Automatic checks stay off until you turn them on.

## Linux and macOS

On Linux, players are detected through MPRIS (mpv needs the `mpv-mpris` script). On macOS, Patchouli asks VLC, QuickTime Player, Safari and Chromium-based browsers through AppleScript. That code builds and passes its tests, but nobody has run it on either system yet.

## Where your data lives

Everything is in `%APPDATA%\app.tracker.desktop`: the database (`media.sqlite`), `settings.json`, `accounts.json`, cached posters and logs. For a portable install, put an empty file named `portable` next to `patchouli.exe` and the data goes in a `data` folder beside it instead.

Once a day Patchouli copies the database, settings, accounts file and torrent archive into `backups\` and keeps the last 7. You can restore one from Settings › Advanced › Backups. It backs up your current data first, then restarts with the backup.
