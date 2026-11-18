# Patchouli — UI brief for a redesign

## What the app is

Patchouli is a Windows desktop app for people who watch anime and keep a list on **AniList** or **MyAnimeList**. It's a modern rewrite of Taiga, a long-running open-source app.

It does three things:

1. **Keeps your anime list.** You can see, edit and score what you're watching, have finished or plan to watch. Changes sync to your AniList or MyAnimeList account and are queued while you're offline.
2. **Notices what you're watching.** It sees the episode playing in a media player (mpv, VLC, MPC-HC…) or a browser, recognises the anime from the file name, and after a short countdown marks the episode as watched.
3. **Helps you find more.** It has search, a browser for each season's new anime, your watch history, your local video folders, and torrent feeds that pick out new episodes of shows you follow.

Users are anime fans who often have lists of hundreds of entries. Many come from Taiga and expect a dense, fast tool rather than a showcase. The app usually runs in the background (system tray) and is opened to check or fix things.

## Constraints

- **A desktop window, not a website.** The default size is 1200 × 780 and the minimum is 800 × 500. It's resizable, with no mobile layout.
- **Light and dark themes,** following Windows or chosen in Settings. Every design needs both.
- **Built with HTML/CSS** (React) inside a native window. Colours, spacing and radii are design tokens (CSS variables), so a new visual language can be applied centrally.
- The current font is Segoe UI (the Windows system font) at 14 px.
- **Poster images** come from AniList or MyAnimeList, at about a 2:3 ratio, and are often missing. A placeholder with the title's first letter shows instead.
- **Three title languages:** romaji, English or Japanese, chosen by the user. Titles can be long and must truncate gracefully.
- **Lists can be long** (500+ rows). The table is virtualised, so rows need a fixed height.
- **Status bar** at the bottom: account and sync state on the left, and what's playing now on the right.

## Layout

```
┌───────────┬──────────────────────────────────────────┐
│ Patchouli │  Page title                     [actions]│
│           │                                          │
│ Home      │  Page content                            │
│ Anime List│                                          │
│ Search    │                                          │
│ Seasons   │                                          │
│ History   │                                          │
│ Library   │                                          │
│ Torrents  │                                          │
│ Profile   │                                          │
│           │                                          │
│ Settings  │                                          │
├───────────┴──────────────────────────────────────────┤
│ AniList: user · syncing…        ▶ Show #5 · 1:30 left│
└──────────────────────────────────────────────────────┘
```

Overlays sit on top of every page:

- the anime dialog;
- right-click menus;
- toasts (bottom);
- native Windows confirmation dialogs.

## Pages

| Page | What it shows | Main interactions |
|---|---|---|
| **Home** | The "Now playing" card, a "Continue watching" grid (posters with progress bars), and prompts such as "Log in" or "Import from Taiga". | Accept or cancel a pending list update, open an anime. |
| **Anime List** | The user's list, with tabs per status: Watching, Completed, On hold, Dropped, Plan to watch. There are two views: a dense **table** (title, progress with ± buttons, score, type, season, dates, last updated) and **poster cards**. | Filter by title or type, sort, +1 episode, change the score, right-click menu, open the anime dialog. |
| **Search** | Search by title, or browse a season. The results are a poster grid with an "in your list" badge. | Search, filter by type or status, add to the list. |
| **Seasons** | Every anime in one season (e.g. "Fall 2026"), grouped into TV, ONA, Movies, and OVA & specials. | Previous / next season, sort, hide anime already in the list. |
| **History** | Episodes watched, grouped by day ("Today", "Yesterday", dates). | Remove an item, clear the history. |
| **Library** | Anime found in the user's video folders, with episode counts and unrecognised files. | Rescan, play the next episode, open a folder. |
| **Torrents** | A table of releases from an RSS feed: anime, episode, fansub group, resolution, size, seeders, release time. Rows can be *selected* (highlighted, to download), *discarded* (dimmed) or *hidden*, and new episodes carry a "New" badge. | Check the feed, search, tick or untick rows, download the selected ones. |
| **Profile** | Avatar and username, stat tiles (anime count, episodes, time watched, time left, mean score), a score-distribution bar chart, an episodes-per-month chart, and breakdowns by status, type and genre. | Open the profile on AniList or MyAnimeList. |
| **Settings** | A section list on the left and forms on the right. The sections are Application, Accounts, Library, Recognition, Media players, Streaming, Sharing, Torrents and Advanced. Torrents contains a **filter editor** (rules made of "element / operator / value" rows). | Toggles, selects, folder pickers, log in or out. |

## Key components

- **Anime dialog** (modal, three tabs):
  - *Details*: poster, titles, synopsis, genres, studios, airing dates, score.
  - *My list*: status, episodes, score, dates, rewatching, notes, private flag.
  - *Settings*: custom title, alternative titles, folder.
- **Now playing card:** the player, the recognised anime and episode, a countdown before the list is updated, and *Update now* / *Cancel*. It must also handle "not recognised" and "update would skip episodes — confirm?".
- **List row / poster card:** title (2-line clamp), progress (`5 / 12` plus a thin progress bar), score (shown in the user's format: 1–10, 1–100, decimals, 5 stars or 3 faces), and badges ("queued" = not yet synced, "airing", "New").
- **Right-click menu:** nested menus with checkmarks, separators and a destructive (red) item.
- **Toast:** a short message at the bottom that disappears after 3.5 s.
- **Status badges and chips:** small, pill-shaped, muted or accent-coloured.

## States to design for

- Not logged in.
- An empty list.
- Loading.
- Offline or sync error (including "login expired").
- Unsynced changes waiting.
- Nothing playing, versus playing but not recognised.
- Long titles, missing posters, anime with an unknown episode count.

## What the redesign may change

Everything visual is open: colour, type, iconography, spacing, density, the sidebar and status bar treatment, cards versus tables, and charts.

Please keep:

- the page set and what each page does;
- light and dark themes;
- a dense table option for long lists;
- keyboard and screen-reader usability (visible focus, sufficient contrast).

To see the current UI with sample data in a browser, a developer can run `npm run ui:dev` and open http://localhost:1420. Screenshots can be taken from there.

## Current style direction

The app now follows the mock-up in `ui-reference/`, minus its decoration. All values are tokens at the top of `ui/src/styles.css`.

What was taken from the reference:

- **Colour.** Violet-tinted neutrals: near-black `#0f0e17` with panels `#191726` in dark, lavender `#f1eff9` with white panels in light. One violet accent (`#9b7bff` dark, `#633de0` light), with a stronger step `#7c56f6` / `#512dc7` for filled buttons. Green, gold, amber and red are kept for status only.
- **Surfaces.** Three panel levels, 1 px lines in two strengths, radii of 6, 10 and 14 px, and one soft drop shadow on panels.
- **Navigation.** A 212 px sidebar grouped into *Tracking*, *Discover & local* and *Account*, with small uppercase group labels and count badges. The active item has a tinted fill and an accent outline.
- **Data.** Tables with small uppercase column headers, a thin progress bar under each count, and tabular figures. Stat tiles show the label above a large value. Status tabs carry count chips.
- **Badges.** Small, squared (5 px radius), with tinted backgrounds.
- **Type.** Segoe UI (the reference also uses it first on Windows), with a monospace for file names and code.

What was deliberately left out, because it is decoration that hurts a dense everyday tool:

| Reference feature | Why it's out |
|---|---|
| Radial-gradient "glow" washes behind the whole app | Noise behind content; lowers contrast unevenly |
| Gradient fills on buttons, avatars, the logo, progress bars and chart bars (violet→pink, violet→teal) | Colour stops carry no meaning; flat fills read faster and theme cleanly |
| Gradient-faded alert banners | Tinted background and border say the same thing |
| Glowing status dots and pulsing animations | Constant motion in a tray app is distracting |
| Glassmorphism (`backdrop-filter: blur`) on overlays, toasts and the status bar | Costly to render; text over blur is harder to read |
| Inset white highlights on cards; text shadows on posters | Simulated depth with no function |
| Cards lifting on hover; dialogs that scale in | Movement that slows down repeated use |
| A fake title bar (mac-style window dots, version pill, language switch) | Windows already draws the title bar; this duplicates it |
| Themed copy ("Grimoire Sync Engine", "Airing radar", "Rating curve", "Activity cadence", "Active rotation") | Cute labels hide what things are; plain names are kept |
| Accent-coloured uppercase eyebrow labels on every card | Too many accents; section labels stay muted |
| Decorative sparklines and badges such as "Top 8%" or "+3 this wk" in stat tiles | Invented metrics with no source |
| A badge on every row ("AIRING", "DISK") | Badge overload; badges are used only where they change a decision |
| Rainbow stacked bars without labels | Unreadable; breakdowns use labelled single-colour bars |
| Web fonts from Google Fonts | Needs the network and the app's content policy blocks it; system fonts are used |
