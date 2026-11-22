# Patchouli

Patchouli is a desktop anime tracker. It recognizes episodes playing on your computer and updates your AniList or MyAnimeList progress. It can also scan your local library and open the next unwatched episode.

## Getting started

Download a Windows installer or portable ZIP from [GitHub Releases](https://github.com/oguzaliarslan/patchouli/releases). If there are no packages listed, see [building from source](docs/development.md).

1. Add your anime folders under **Settings ? Library**.
2. Connect AniList or MyAnimeList under **Settings ? Accounts**. If the build needs a client ID, follow the [account setup guide](docs/setup.md).
3. Play an episode in a supported player. **Home** shows what Patchouli recognized and when it will update your list.

If you're coming from Taiga, use **Settings ? Advanced ? Import from Taiga** to bring over your list, history, and settings. Reconnect your account after importing.

## What else it does

- See when the next episode of each show you're watching airs, and get a notification when it does.
- Keep both an AniList and a MyAnimeList list: changes to your main list are copied to the other one.
- Find sequels of what you've finished that aren't in your list, and recommendations based on what you liked.
- Browse seasonal anime and see your watch history and list statistics.
- Check torrent RSS feeds and filter releases against your list.
- Show what you're watching on Discord or send playback events to a webhook.
- Export your list as Markdown or MyAnimeList XML.
- Press Ctrl+K to jump to any page or anime. System-wide shortcuts record the next episode of what's playing (Ctrl+Alt+Shift+Up) or pause detection (Ctrl+Alt+Shift+P).

Windows is the tested platform. Linux and macOS player detection are experimental.

## Documentation

- [Accounts, Discord, and local data](docs/setup.md)
- [Development and troubleshooting](docs/development.md)
- [Windows packaging and updates](docs/releases.md)
- [Migration notes](MIGRATION_PLAN.md)

Patchouli is a Rust and Tauri port of [Taiga](https://github.com/erengy/taiga). See the [GPLv3 license](LICENSE) and [third-party notices](THIRD_PARTY.md).
