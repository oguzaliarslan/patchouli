//! What each torrent site puts where (Taiga v1 `track/feed_source.cpp`).

use crate::item::{Category, FeedItem, parse_size};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Source {
    #[default]
    Unknown,
    Acgnx,
    AniDex,
    AnimeBytes,
    AnimeTosho,
    Minglong,
    NyaaPantsu,
    NyaaSi,
    SubsPlease,
    TokyoToshokan,
}

/// The site, from the channel's link (or the feed URL).
pub fn detect(link: &str) -> Source {
    const SOURCES: [(&str, Source); 11] = [
        ("acgnx", Source::Acgnx),
        ("anidex", Source::AniDex),
        ("animebytes", Source::AnimeBytes),
        ("animetosho", Source::AnimeTosho),
        ("minglong", Source::Minglong),
        ("nyaa.net", Source::NyaaPantsu),
        ("nyaa.pantsu", Source::NyaaPantsu),
        ("nyaa.pt", Source::NyaaPantsu),
        ("nyaa.si", Source::NyaaSi),
        ("subsplease", Source::SubsPlease),
        ("tokyotosho", Source::TokyoToshokan),
    ];
    let host = host(link).to_ascii_lowercase();
    SOURCES
        .iter()
        .find(|(name, _)| host.contains(name))
        .map_or(Source::Unknown, |&(_, source)| source)
}

fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    authority.rsplit_once('@').map_or(authority, |(_, h)| h)
}

/// The text between `start` and `end` (or the end of the text if `end` is
/// empty), like Taiga's `InStr(text, start, end)`.
fn between<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
    let Some(pos) = text.find(start) else {
        return "";
    };
    let rest = &text[pos + start.len()..];
    if end.is_empty() {
        return rest;
    }
    rest.find(end).map_or("", |e| &rest[..e])
}

/// Fills in links, size and counts from the site's own fields. Runs before
/// the description is tidied, since magnet links are found in its HTML.
pub fn apply(source: Source, item: &mut FeedItem) {
    let parse_magnet = |item: &mut FeedItem| {
        if item.enclosure_url.starts_with("magnet:") {
            item.magnet_link = item.enclosure_url.clone();
        } else {
            let magnet = between(&item.description, "<a href=\"magnet:?", "\">");
            if !magnet.is_empty() {
                item.magnet_link = format!("magnet:?{magnet}");
            }
        }
    };
    match source {
        Source::Acgnx => {
            if let Some(size) = item.description.split(" | ").nth(2) {
                item.file_size = parse_size(size);
            }
            parse_magnet(item);
        }
        Source::AniDex => {
            item.file_size = parse_size(between(&item.description, "Size: ", " |"));
            if item.description.contains(" Batch ") {
                item.category = Category::Batch;
            }
            parse_magnet(item);
        }
        Source::AnimeTosho => {
            if item.link.contains("/view/") && !item.enclosure_url.is_empty() {
                item.link = item.enclosure_url.clone();
            }
            item.info_link = item.guid.clone();
            item.file_size = parse_size(between(
                &item.description,
                "<strong>Total Size</strong>: ",
                "<",
            ));
            parse_magnet(item);
        }
        Source::Minglong => {
            item.file_size = parse_size(between(&item.description, "Size: ", "<"));
        }
        Source::NyaaPantsu => {
            item.info_link = item.guid.clone();
            item.file_size = item.enclosure_length.trim().parse().unwrap_or(0);
        }
        Source::NyaaSi => {
            item.info_link = item.guid.clone();
            let elements = &item.namespace_elements;
            let number = |key: &str| elements.get(key).and_then(|v| v.trim().parse().ok());
            if let Some(size) = elements.get("nyaa:size") {
                item.file_size = parse_size(size);
            }
            item.seeders = number("nyaa:seeders");
            item.leechers = number("nyaa:leechers");
            item.downloads = number("nyaa:downloads");
        }
        Source::SubsPlease => {
            if let Some(size) = item.namespace_elements.get("subsplease:size") {
                item.file_size = parse_size(size);
            }
        }
        Source::TokyoToshokan => {
            item.file_size = parse_size(between(&item.description, "Size: ", "<"));
            parse_magnet(item);
            item.description = between(&item.description, "Comment: ", "").to_owned();
            item.info_link = item.guid.clone();
        }
        Source::AnimeBytes | Source::Unknown => {}
    }
}

/// Taiga's `TidyFeedItemDescription`: HTML to one line, parts joined by ` | `.
pub fn tidy_description(description: &str) -> String {
    static TAGS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"<[^>]*>").expect("valid pattern"));
    let text = description
        .replace("</p>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<br>", "\n");
    let text = TAGS.replace_all(&text, "");
    let text = text
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");
    text.split('\n')
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" | ")
}

/// AnimeBytes titles (`Title - TV Series [2020] :: Group ...`) don't parse
/// as is; keep the title, mark where it ends, and keep the release details.
pub fn preprocess_title(source: Source, title: &str) -> String {
    if source != Source::AnimeBytes {
        return title.to_owned();
    }
    static PATTERN: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^(.+) - .+ \[\d{4}\] :: (.+)$").expect("valid pattern")
    });
    match PATTERN.captures(title) {
        Some(caps) => format!("{} [REMASTER] {}", &caps[1], &caps[2]),
        None => title.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_sources() {
        assert_eq!(detect("https://nyaa.si/?page=rss"), Source::NyaaSi);
        assert_eq!(
            detect("https://www.tokyotosho.info/rss.php"),
            Source::TokyoToshokan
        );
        assert_eq!(detect("http://example.com/nyaa.si"), Source::Unknown);
    }

    #[test]
    fn tokyotosho_items() {
        let mut item = FeedItem {
            description: "<a href=\"magnet:?xt=urn:btih:ABC\">Magnet Link</a><br />Size: 350.5MB<br />Comment: Hello &amp; welcome".into(),
            guid: "https://www.tokyotosho.info/details.php?id=1".into(),
            ..FeedItem::default()
        };
        apply(Source::TokyoToshokan, &mut item);
        assert_eq!(item.magnet_link, "magnet:?xt=urn:btih:ABC");
        assert_eq!(item.file_size, 350_500_000);
        assert_eq!(tidy_description(&item.description), "Hello & welcome");
        assert_eq!(item.info_link, item.guid);
    }

    #[test]
    fn tidies_descriptions() {
        assert_eq!(
            tidy_description("<p>One  two</p><p></p>Three<br/>  four "),
            "One two | Three | four"
        );
    }

    #[test]
    fn animebytes_titles() {
        assert_eq!(
            preprocess_title(
                Source::AnimeBytes,
                "Show - TV Series [2020] :: Blu-ray | MKV | 1080p"
            ),
            "Show [REMASTER] Blu-ray | MKV | 1080p"
        );
    }
}
