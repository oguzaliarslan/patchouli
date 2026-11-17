# Third-party files

Copied from [erengy/taiga](https://github.com/erengy/taiga) (`develop` branch) and its submodules on 2026-11-17.

| Path | Source | License |
|---|---|---|
| `crates/db/sql/*.sql` | `taiga/src/resources/sql` | GPL-3.0 |
| `crates/sync/gql/anilist/*.gql` | `taiga/src/resources/gql/anilist` | GPL-3.0 |
| `src-tauri/resources/players.anisthesia` | `erengy/anisthesia/data` | Public domain |
| `crates/anitomy/src/*` | Rust port of `erengy/anitomy` (`include/anitomy`) | MPL-2.0 |
| `crates/anitomy/tests/oracle/inputs.txt`, `tests/data/golden.jsonl` | Inputs from `erengy/anitomy/test/data.json`, outputs recorded from the C++ library | MPL-2.0 |
| `crates/detect/src/*` | Rust port of `erengy/anisthesia` (`src/player.cpp`, `src/win_*.cpp`) | MIT |
| `crates/track/src/*` | Port of Taiga `src/track/media.cpp`, `update_decision.cpp`, `update_session.cpp` | GPL-3.0 |
| `crates/share/src/*`, `src-tauri/src/sharing.rs`, `src-tauri/src/export.rs` | Port of Taiga `src/share/*` and `src/media/anime_list_export.cpp` | GPL-3.0 |
| `crates/recognition/src/*` | Port of Taiga `src/track/recognition*.cpp`, `scanner.cpp`, `play.cpp` | GPL-3.0 |
| `src-tauri/resources/anime-relations.txt` | `erengy/anime-relations` | Public domain |

If the project chooses a clean-room, non-GPL license (MIGRATION_PLAN.md §3), the GPL-3.0 files above must be rewritten rather than copied.
