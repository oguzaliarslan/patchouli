//! Taiga v1's XML files (Taiga `src/compat/*.cpp`):
//!
//! - `settings.xml`: settings, the active service and username, library
//!   folders, per-anime folders and titles
//! - `db/anime.xml`: anime, with an ID per service
//! - `user/<username>@<service>/anime.xml`: the list
//! - `user/<username>@<service>/history.xml`: history and unsent changes

use std::collections::HashMap;
use std::path::Path;

use serde_json::{Map, Value, json};
use tracker_core::anime::{AgeRating, Status, Titles, Type};
use tracker_core::feed::{Condition, Element as Element_, Filter, Operator};
use tracker_core::{
    Anime, AnimeSettings, Fields, FuzzyDate, HistoryItem, ListEntry, ListStatus, QueueItem,
    ServiceId,
};

use crate::xml::{Element, parse, remove_invalid_character_references};
use crate::{ImportData, read_text};

fn root(path: &Path, name: &str) -> Result<Element, String> {
    let text = read_text(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let text = remove_invalid_character_references(&text);
    parse(&text)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .into_iter()
        .find(|e| e.name == name)
        .ok_or_else(|| format!("{}: no <{name}> element", path.display()))
}

fn int(text: &str) -> i64 {
    text.trim().parse().unwrap_or(0)
}

fn list(text: &str, separator: &str) -> Vec<String> {
    text.split(separator)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn read(dir: &Path) -> Result<ImportData, String> {
    let mut data = ImportData::default();

    let settings_path = dir.join("settings.xml");
    let settings = if settings_path.is_file() {
        match root(&settings_path, "settings") {
            Ok(settings) => Some(settings),
            Err(e) => {
                data.warnings.push(e);
                None
            }
        }
    } else {
        data.warnings
            .push("settings.xml is missing; using MyAnimeList.".into());
        None
    };

    // v1 defaulted to MyAnimeList.
    let slug = settings
        .as_ref()
        .and_then(|s| s.child("account")?.child("update"))
        .map(|u| u.attr("activeservice").to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "myanimelist".into());
    let service =
        ServiceId::from_slug(&slug).ok_or_else(|| format!("Unknown service \"{slug}\"."))?;
    data.service = Some(service);
    data.username = settings
        .as_ref()
        .and_then(|s| s.child("account")?.child(&slug))
        .map(|a| a.attr("username").to_owned())
        .filter(|u| !u.is_empty());
    if let Some(settings) = &settings {
        data.settings = read_settings(settings, &slug);
        data.anime_settings = read_anime_settings(settings);
    }

    let anime_path = dir.join("db").join("anime.xml");
    match root(&anime_path, "database") {
        Ok(database) => data.anime = read_anime(&database, &slug),
        Err(e) => data.warnings.push(e),
    }

    let Some(user_dir) = user_folder(dir, data.username.as_deref(), &slug) else {
        data.warnings.push(format!(
            "No {} list was found under user\\.",
            service.name()
        ));
        return Ok(data);
    };
    if data.username.is_none() {
        data.username = user_dir
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.rsplit_once('@'))
            .map(|(name, _)| name.to_owned());
    }
    match root(&user_dir.join("anime.xml"), "library") {
        Ok(library) => data.entries = read_entries(&library),
        Err(e) => data.warnings.push(e),
    }
    let history_path = user_dir.join("history.xml");
    if history_path.is_file() {
        match root(&history_path, "history") {
            Ok(history) => {
                data.history = read_history(&history);
                apply_queue(&history, &mut data.entries, &mut data.queue);
            }
            Err(e) => data.warnings.push(e),
        }
    }
    Ok(data)
}

/// `user/<username>@<service>`, or the only folder for the service.
fn user_folder(dir: &Path, username: Option<&str>, slug: &str) -> Option<std::path::PathBuf> {
    let users = dir.join("user");
    if let Some(name) = username {
        let path = users.join(format!("{name}@{slug}"));
        if path.is_dir() {
            return Some(path);
        }
    }
    let suffix = format!("@{slug}");
    let mut matches: Vec<_> = std::fs::read_dir(&users)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(&suffix))
        })
        .collect();
    matches.sort();
    (matches.len() == 1).then(|| matches.remove(0))
}

fn read_settings(settings: &Element, slug: &str) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert("sync.service".into(), json!(slug));
    if let Some(update) = settings.child("account").and_then(|a| a.child("update")) {
        if update.has_attr("delay") {
            map.insert("track.update.delay".into(), json!(update.attr_int("delay")));
        }
        if update.has_attr("checkplayer") {
            map.insert(
                "track.update.pauseWhenUnfocused".into(),
                json!(update.attr_bool("checkplayer")),
            );
        }
        if update.has_attr("outofroot") {
            map.insert(
                "track.update.libraryOnly".into(),
                json!(update.attr_bool("outofroot")),
            );
        }
        if update.has_attr("waitplayer") {
            let trigger = if update.attr_bool("waitplayer") {
                "onPlayerClose"
            } else {
                "afterDelay"
            };
            map.insert("track.update.trigger".into(), json!(trigger));
        }
    }
    if let Some(folders) = settings.child("anime").and_then(|a| a.child("folders")) {
        let folders: Vec<&str> = folders
            .children_named("root")
            .map(|r| r.attr("folder"))
            .filter(|f| !f.is_empty())
            .collect();
        map.insert("library.folders".into(), json!(folders));
    }
    if let Some(recognition) = settings.child("recognition") {
        if let Some(players) = recognition.child("mediaplayers") {
            let disabled: Vec<&str> = players
                .children_named("player")
                .filter(|p| !p.attr_bool("enabled"))
                .map(|p| p.attr("name"))
                .collect();
            map.insert("track.detection.disabledPlayers".into(), json!(disabled));
        }
        if let Some(general) = recognition
            .child("general")
            .filter(|g| g.has_attr("detectioninterval"))
        {
            let seconds = general.attr_int("detectioninterval").max(1);
            map.insert("track.detection.interval".into(), json!(seconds * 1000));
        }
        if let Some(streaming) = recognition
            .child("streaming")
            .filter(|s| s.has_attr("enabled"))
        {
            map.insert(
                "track.detection.streaming.enabled".into(),
                json!(streaming.attr_bool("enabled")),
            );
        }
    }
    if let Some(torrent) = settings.child("rss").and_then(|r| r.child("torrent")) {
        read_torrent_settings(torrent, &mut map);
    }
    if let Some(discord) = settings
        .child("announce")
        .and_then(|a| a.child("discord"))
        .filter(|d| d.has_attr("enabled"))
    {
        map.insert(
            "sharing.discord.enabled".into(),
            json!(discord.attr_bool("enabled")),
        );
    }
    if let Some(program) = settings.child("program") {
        if let Some(action) = program.child("list").and_then(|l| l.child("action")) {
            let language = match action.attr("titlelang") {
                "english" => "english",
                "native" => "native",
                _ => "romaji",
            };
            map.insert("app.titleLanguage".into(), json!(language));
        }
        if let Some(general) = program
            .child("general")
            .filter(|g| g.has_attr("enablesharing"))
        {
            map.insert(
                "sharing.enabled".into(),
                json!(general.attr_bool("enablesharing")),
            );
        }
        if let Some(proxy) = program
            .child("proxy")
            .filter(|p| !p.attr("host").is_empty())
        {
            let raw = proxy.attr("host");
            let url = if raw.contains("://") {
                raw.to_owned()
            } else {
                format!("http://{raw}")
            };
            let (scheme, rest) = url.split_once("://").unwrap_or(("http", raw));
            let authority = rest.split('/').next().unwrap_or_default();
            let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
            let (host, port) = match host_port.rsplit_once(':') {
                Some((host, port)) => (host, port.parse::<i64>().unwrap_or(-1)),
                None => (host_port, -1),
            };
            let kind = if scheme.to_lowercase().contains("socks5") {
                "socks5"
            } else {
                "http"
            };
            map.insert("network.proxy.type".into(), json!(kind));
            map.insert("network.proxy.host".into(), json!(host));
            map.insert("network.proxy.port".into(), json!(port));
            map.insert(
                "network.proxy.username".into(),
                json!(proxy.attr("username")),
            );
            map.insert(
                "network.proxy.password".into(),
                json!(proxy.attr("password")),
            );
        }
    }
    map
}

/// `rss/torrent/*`: the feed, downloads and filters.
fn read_torrent_settings(torrent: &Element, map: &mut Map<String, Value>) {
    let mut set = |key: &str, value: Value| {
        map.insert(key.into(), value);
    };
    if let Some(address) = torrent
        .child("source")
        .map(|s| s.attr("address"))
        .filter(|a| !a.is_empty())
    {
        set("torrents.source", json!(address));
    }
    if let Some(address) = torrent
        .child("search")
        .map(|s| s.attr("address"))
        .filter(|a| !a.is_empty())
    {
        set("torrents.searchUrl", json!(address));
    }
    if let Some(options) = torrent.child("options") {
        let flag = |name: &str| {
            options
                .has_attr(name)
                .then(|| json!(options.attr_bool(name)))
        };
        let text = |name: &str| options.has_attr(name).then(|| json!(options.attr(name)));
        for (key, value) in [
            ("torrents.autoCheck.enabled", flag("autocheck")),
            ("torrents.download.folder", text("downloadpath")),
            ("torrents.download.fileFolder", text("filedownloadpath")),
            ("torrents.download.useAnimeFolder", flag("autosetfolder")),
            ("torrents.download.fallbackToFolder", flag("autousefolder")),
            (
                "torrents.download.createSubfolder",
                flag("autocreatefolder"),
            ),
            ("torrents.download.useMagnet", flag("downloadusemagnet")),
        ] {
            if let Some(value) = value {
                set(key, value);
            }
        }
        if options.has_attr("checkinterval") {
            set(
                "torrents.autoCheck.interval",
                json!(options.attr_int("checkinterval")),
            );
        }
        if options.has_attr("newaction") {
            let action = if options.attr_int("newaction") == 2 {
                "download"
            } else {
                "notify"
            };
            set("torrents.autoCheck.action", json!(action));
        }
        if options.has_attr("downloadsortby") {
            let by = if options.attr("downloadsortby") == "release_date" {
                "releaseDate"
            } else {
                "episodeNumber"
            };
            set("torrents.download.sortBy", json!(by));
        }
        if options.has_attr("downloadsortorder") {
            set(
                "torrents.download.descending",
                json!(options.attr("downloadsortorder") == "descending"),
            );
        }
    }
    if let Some(app) = torrent.child("application") {
        if app.has_attr("mode") {
            let mode = if app.attr_int("mode") == 2 {
                "custom"
            } else {
                "default"
            };
            set("torrents.app.mode", json!(mode));
        }
        if app.has_attr("open") {
            set("torrents.app.open", json!(app.attr_bool("open")));
        }
        if app.has_attr("path") {
            set("torrents.app.path", json!(app.attr("path")));
        }
    }
    if let Some(filter) = torrent.child("filter") {
        if filter.has_attr("enabled") {
            set(
                "torrents.filter.enabled",
                json!(filter.attr_bool("enabled")),
            );
        }
        if filter.has_attr("archive_maxcount") {
            set(
                "torrents.filter.archiveMaxCount",
                json!(filter.attr_int("archive_maxcount").max(0)),
            );
        }
        let filters: Vec<Filter> = filter
            .children_named("item")
            .filter_map(read_filter)
            .collect();
        if filter.children_named("item").next().is_some() {
            set("torrents.filters", json!(filters));
        }
    }
}

fn shortcode<T: serde::de::DeserializeOwned>(value: &str) -> Option<T> {
    serde_json::from_value(json!(value.to_lowercase())).ok()
}

/// A filter in Taiga's shortcodes. Filters with unknown parts are skipped.
fn read_filter(item: &Element) -> Option<Filter> {
    let conditions = item
        .children_named("condition")
        .map(|c| {
            Some(Condition {
                element: Element_::from_shortcode(c.attr("element"))?,
                operator: Operator::from_shortcode(c.attr("operator"))?,
                value: c.attr("value").to_owned(),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Filter {
        name: item.attr("name").to_owned(),
        enabled: item.attr_bool("enabled"),
        action: shortcode(item.attr("action"))?,
        matching: shortcode(item.attr("match"))?,
        option: shortcode(item.attr("option")).unwrap_or_default(),
        anime_ids: item
            .children_named("anime")
            .map(|a| a.attr_int("id") as i32)
            .filter(|&id| id != 0)
            .collect(),
        conditions,
    })
}

fn read_anime_settings(settings: &Element) -> Vec<AnimeSettings> {
    let Some(items) = settings.child("anime").and_then(|a| a.child("items")) else {
        return Vec::new();
    };
    items
        .children_named("item")
        .filter_map(|item| {
            let id = item.attr_int("id") as i32;
            let folder = item.attr("folder").to_owned();
            let synonyms = list(item.attr("titles"), "; ");
            (id != 0 && (!folder.is_empty() || !synonyms.is_empty())).then(|| AnimeSettings {
                id,
                display_title: String::new(),
                folder,
                synonyms,
            })
        })
        .collect()
}

/// Anime with the current service's ID. Each `<id>` names its service.
fn read_anime(database: &Element, slug: &str) -> Vec<Anime> {
    database
        .children_named("anime")
        .filter_map(|node| {
            let ids: Vec<&Element> = node.children_named("id").collect();
            let id = ids
                .iter()
                .find(|i| i.attr("name") == slug)
                .or_else(|| ids.iter().find(|i| !i.has_attr("name")))
                .map(|i| int(&i.text) as i32)
                .filter(|&id| id != 0)?;
            Some(Anime {
                id,
                last_modified: int(node.text_of("modified")),
                episode_count: node
                    .child("episode_count")
                    .map_or(-1, |e| int(&e.text) as i32),
                episode_length: node
                    .child("episode_length")
                    .map_or(-1, |e| int(&e.text) as i32),
                age_rating: AgeRating::from_i32(int(node.text_of("age_rating")) as i32)
                    .unwrap_or_default(),
                status: Status::from_i32(int(node.text_of("status")) as i32).unwrap_or_default(),
                r#type: Type::from_i32(int(node.text_of("type")) as i32).unwrap_or_default(),
                date_started: FuzzyDate::parse(node.text_of("date_start")),
                date_finished: FuzzyDate::parse(node.text_of("date_end")),
                score: node.text_of("score").trim().parse().unwrap_or(0.0),
                popularity_rank: int(node.text_of("popularity")) as i32,
                image_url: node.text_of("image").to_owned(),
                slug: node.text_of("slug").to_owned(),
                synopsis: node.text_of("synopsis").to_owned(),
                trailer_id: node.text_of("trailer_id").to_owned(),
                titles: Titles {
                    romaji: node.text_of("title").to_owned(),
                    english: node.text_of("english").to_owned(),
                    japanese: node.text_of("japanese").to_owned(),
                    synonyms: node
                        .children_named("synonym")
                        .map(|s| s.text.clone())
                        .filter(|s| !s.is_empty())
                        .collect(),
                },
                genres: list(node.text_of("genres"), ", "),
                producers: list(node.text_of("producers"), ", "),
                studios: list(node.text_of("studios"), ", "),
                tags: list(node.text_of("tags"), ", "),
                last_aired_episode: int(node.text_of("last_aired_episode")) as i32,
                next_episode_time: int(node.text_of("next_episode_time")),
            })
        })
        .collect()
}

fn read_entries(library: &Element) -> Vec<ListEntry> {
    library
        .children_named("anime")
        .filter_map(|node| {
            let anime_id = int(node.text_of("id")) as i32;
            (anime_id != 0).then(|| ListEntry {
                id: int(node.text_of("library_id")),
                anime_id,
                watched_episodes: int(node.text_of("progress")) as i32,
                score: int(node.text_of("score")) as i32,
                status: ListStatus::from_i32(int(node.text_of("status")) as i32)
                    .unwrap_or_default(),
                is_private: int(node.text_of("private")) != 0,
                rewatched_times: int(node.text_of("rewatched_times")) as i32,
                rewatching: int(node.text_of("rewatching")) != 0,
                rewatching_ep: int(node.text_of("rewatching_ep")) as i32,
                date_started: FuzzyDate::parse(node.text_of("date_start")),
                date_completed: FuzzyDate::parse(node.text_of("date_end")),
                last_updated: int(node.text_of("last_updated")),
                notes: node.text_of("notes").to_owned(),
                pending_delete: false,
            })
        })
        .collect()
}

/// `yyyy-MM-dd HH:mm:ss` in local time.
fn parse_time(value: &str) -> i64 {
    use chrono::TimeZone;
    chrono::NaiveDateTime::parse_from_str(value.trim(), "%Y-%m-%d %H:%M:%S")
        .ok()
        .and_then(|t| chrono::Local.from_local_datetime(&t).earliest())
        .map_or(0, |t| t.timestamp())
}

fn read_history(history: &Element) -> Vec<HistoryItem> {
    let mut items: Vec<HistoryItem> = history
        .children_named("items")
        .flat_map(|items| items.children_named("item"))
        .map(|item| HistoryItem {
            id: 0,
            anime_id: item.attr_int("anime_id") as i32,
            episode: item.attr_int("episode") as i32,
            time: parse_time(item.attr("time")),
        })
        .filter(|h| h.anime_id != 0)
        .collect();
    items.sort_by_key(|h| h.time);
    items
}

/// Applies v1's unsent changes to the entries and queues them again
/// (Taiga's `Queue::migrateFromV1`).
fn apply_queue(history: &Element, entries: &mut Vec<ListEntry>, queue: &mut Vec<QueueItem>) {
    let now = chrono::Utc::now().timestamp();
    let mut queued: HashMap<i32, Fields> = HashMap::new();
    for item in history
        .children_named("queue")
        .flat_map(|q| q.children_named("item"))
    {
        let anime_id = item.attr_int("anime_id") as i32;
        if anime_id == 0 {
            continue;
        }
        let index = match entries.iter().position(|e| e.anime_id == anime_id) {
            Some(index) => index,
            None => {
                entries.push(ListEntry {
                    anime_id,
                    ..ListEntry::default()
                });
                entries.len() - 1
            }
        };
        let entry = &mut entries[index];
        if item.attr("mode") == "delete" {
            entry.pending_delete = true;
            queued.insert(anime_id, Fields::empty());
            continue;
        }
        let mut dirty = Fields::empty();
        if item.has_attr("episode") {
            entry.watched_episodes = item.attr_int("episode") as i32;
            dirty |= Fields::EPISODE;
        }
        if item.has_attr("score") {
            entry.score = item.attr_int("score") as i32;
            dirty |= Fields::SCORE;
        }
        if item.has_attr("status") {
            entry.status = ListStatus::from_i32(item.attr_int("status") as i32).unwrap_or_default();
            dirty |= Fields::STATUS;
        }
        if item.has_attr("enable_rewatching") {
            entry.rewatching = item.attr_bool("enable_rewatching");
            dirty |= Fields::REWATCHING;
        }
        if item.has_attr("rewatched_times") {
            entry.rewatched_times = item.attr_int("rewatched_times") as i32;
            dirty |= Fields::REWATCHED_TIMES;
        }
        if item.has_attr("notes") {
            entry.notes = item.attr("notes").to_owned();
            dirty |= Fields::NOTES;
        }
        if item.has_attr("date_start") {
            entry.date_started = FuzzyDate::parse(item.attr("date_start"));
            dirty |= Fields::DATE_STARTED;
        }
        if item.has_attr("date_finish") {
            entry.date_completed = FuzzyDate::parse(item.attr("date_finish"));
            dirty |= Fields::DATE_COMPLETED;
        }
        entry.pending_delete = false;
        *queued.entry(anime_id).or_default() |= dirty;
    }
    let mut items: Vec<QueueItem> = queued
        .into_iter()
        .map(|(anime_id, dirty)| QueueItem {
            anime_id,
            dirty,
            time: now,
            retry_count: 0,
            last_error: None,
        })
        .collect();
    items.sort_by_key(|q| q.anime_id);
    queue.extend(items);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Taiga v1 data folder with AniList as the active service.
    fn sample(dir: &Path) {
        let write = |path: &str, text: &str| {
            let path = dir.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "settings.xml",
            r#"<?xml version="1.0" encoding="utf-8"?>
<settings>
  <account>
    <update activeservice="anilist" delay="90" checkplayer="true" outofroot="false" waitplayer="true" />
    <anilist username="someone" token="secret-token" />
    <myanimelist username="other" />
  </account>
  <anime>
    <folders><root folder="D:\Anime" /><root folder="E:\More Anime" /></folders>
    <items><item id="21" folder="D:\Anime\One Piece" titles="OP; Wan Pisu" /><item id="0" folder="x" /></items>
  </anime>
  <recognition>
    <general detectioninterval="5" />
    <mediaplayers><player name="mpv" enabled="true" /><player name="VLC media player" enabled="false" /></mediaplayers>
    <streaming enabled="true" />
  </recognition>
  <program>
    <general enablesharing="true" />
    <list><action titlelang="english" /></list>
    <proxy host="socks5://proxy.local:1080" username="u" password="p" />
  </program>
  <announce><discord enabled="true" /><mirc channels="&#03;4#anime" /></announce>
  <rss><torrent>
    <source address="https://nyaa.si/?page=rss" />
    <options autocheck="true" checkinterval="30" newaction="2" downloadsortorder="descending" />
    <application mode="2" open="true" path="C:\qbittorrent.exe" />
    <filter enabled="true" archive_maxcount="500">
      <item action="prefer" match="all" option="default" enabled="true" name="[Fansub] One Piece">
        <anime id="21" /><condition element="episode_group" operator="equals" value="Erai-raws" />
      </item>
      <item action="explode" match="all" option="default" enabled="true" name="Broken" />
    </filter>
  </torrent></rss>
</settings>"#,
        );
        write(
            "db/anime.xml",
            r#"<?xml version="1.0" encoding="utf-8"?>
<meta><version>1.4.1</version></meta>
<database>
  <anime>
    <id name="myanimelist">21</id><id name="anilist">21</id><source>anilist</source>
    <title>One Piece</title><english>One Piece</english><synonym>OP</synonym>
    <type>1</type><status>2</status><episode_count>-1</episode_count><episode_length>24</episode_length>
    <date_start>1999-10-20</date_start><genres>Action, Adventure</genres><score>8.72</score>
  </anime>
  <anime><id name="myanimelist">30</id><title>MAL only</title></anime>
  <anime><id name="anilist">101</id><title>Tom &amp; Jerry</title><episode_count>12</episode_count></anime>
</database>"#,
        );
        write(
            "user/someone@anilist/anime.xml",
            r#"<?xml version="1.0"?>
<meta><version>1.4.1</version></meta>
<library>
  <anime><id>21</id><library_id>5001</library_id><progress>1000</progress><score>85</score><status>1</status>
    <date_start>2020-01-00</date_start><notes>Still going</notes><last_updated>1700000000</last_updated></anime>
  <anime><id>101</id><progress>3</progress><status>1</status><private>1</private></anime>
</library>"#,
        );
        write(
            "user/someone@anilist/history.xml",
            r#"<?xml version="1.0"?>
<history>
  <items>
    <item anime_id="21" episode="1000" time="2024-05-01 21:30:00" />
    <item anime_id="21" episode="999" time="2024-04-30 21:30:00" />
  </items>
  <queue>
    <item anime_id="21" episode="1001" mode="update" />
    <item anime_id="21" score="90" mode="update" />
    <item anime_id="101" mode="delete" />
    <item anime_id="202" episode="1" status="1" mode="add" />
  </queue>
</history>"#,
        );
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("tracker-v1-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reads_a_v1_data_folder() {
        let dir = temp("full");
        sample(&dir);
        let data = read(&dir).unwrap();
        assert!(data.warnings.is_empty(), "{:?}", data.warnings);
        assert_eq!(data.service, Some(ServiceId::AniList));
        assert_eq!(data.username.as_deref(), Some("someone"));

        // Anime with an AniList ID only; `&amp;` unescaped.
        let ids: Vec<i32> = data.anime.iter().map(|a| a.id).collect();
        assert_eq!(ids, [21, 101]);
        let op = &data.anime[0];
        assert_eq!(op.titles.synonyms, ["OP"]);
        assert_eq!(op.genres, ["Action", "Adventure"]);
        assert_eq!(op.r#type, Type::Tv);
        assert_eq!(op.date_started, FuzzyDate::new(1999, 10, 20));
        assert_eq!(data.anime[1].titles.romaji, "Tom & Jerry");

        // The queue is applied on top of the list.
        let op_entry = data.entries.iter().find(|e| e.anime_id == 21).unwrap();
        assert_eq!(op_entry.id, 5001);
        assert_eq!(op_entry.watched_episodes, 1001);
        assert_eq!(op_entry.score, 90);
        assert_eq!(op_entry.date_started, FuzzyDate::new(2020, 1, 0));
        assert!(
            data.entries
                .iter()
                .find(|e| e.anime_id == 101)
                .unwrap()
                .pending_delete
        );
        assert!(
            data.entries
                .iter()
                .any(|e| e.anime_id == 202 && e.status == ListStatus::Watching)
        );
        let dirty: HashMap<i32, Fields> =
            data.queue.iter().map(|q| (q.anime_id, q.dirty)).collect();
        assert_eq!(dirty[&21], Fields::EPISODE | Fields::SCORE);
        assert_eq!(dirty[&101], Fields::empty());
        assert_eq!(dirty[&202], Fields::EPISODE | Fields::STATUS);

        // History, oldest first.
        assert_eq!(
            data.history.iter().map(|h| h.episode).collect::<Vec<_>>(),
            [999, 1000]
        );
        assert!(data.history[0].time > 1_700_000_000);

        // Per-anime settings (ID 0 skipped).
        assert_eq!(data.anime_settings.len(), 1);
        assert_eq!(data.anime_settings[0].synonyms, ["OP", "Wan Pisu"]);

        // Settings in v2 keys; the control character reference didn't break parsing.
        let s = &data.settings;
        assert_eq!(s["sync.service"], "anilist");
        assert_eq!(s["track.update.delay"], 90);
        assert_eq!(s["track.update.trigger"], "onPlayerClose");
        assert_eq!(s["track.update.pauseWhenUnfocused"], true);
        assert_eq!(s["library.folders"], json!(["D:\\Anime", "E:\\More Anime"]));
        assert_eq!(
            s["track.detection.disabledPlayers"],
            json!(["VLC media player"])
        );
        assert_eq!(s["track.detection.interval"], 5000);
        assert_eq!(s["track.detection.streaming.enabled"], true);
        assert_eq!(s["sharing.discord.enabled"], true);
        assert_eq!(s["app.titleLanguage"], "english");
        assert_eq!(s["network.proxy.type"], "socks5");
        assert_eq!(s["network.proxy.host"], "proxy.local");
        assert_eq!(s["network.proxy.port"], 1080);
        assert_eq!(s["torrents.source"], "https://nyaa.si/?page=rss");
        assert_eq!(s["torrents.autoCheck.interval"], 30);
        assert_eq!(s["torrents.autoCheck.action"], "download");
        assert_eq!(s["torrents.download.descending"], true);
        assert_eq!(s["torrents.app.mode"], "custom");
        assert_eq!(s["torrents.filter.archiveMaxCount"], 500);
        let filters = s["torrents.filters"].as_array().unwrap();
        assert_eq!(filters.len(), 1, "the broken filter is skipped");
        assert_eq!(filters[0]["animeIds"], json!([21]));
        assert_eq!(filters[0]["conditions"][0]["element"], "episode_group");
        // Tokens are never imported.
        assert!(!serde_json::to_string(s).unwrap().contains("secret-token"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_files_are_warnings() {
        let dir = temp("empty");
        std::fs::write(dir.join("settings.xml"), "<settings/>").unwrap();
        let data = read(&dir).unwrap();
        assert_eq!(data.service, Some(ServiceId::MyAnimeList));
        assert!(data.anime.is_empty());
        assert!(data.warnings.len() >= 2, "{:?}", data.warnings);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_local_times() {
        assert!(parse_time("2024-05-01 21:30:00") > 1_714_000_000);
        assert_eq!(parse_time("garbage"), 0);
    }
}
