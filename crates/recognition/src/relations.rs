//! Episode redirection rules from `anime-relations.txt`, ported from Taiga
//! `track/recognition_relations.cpp`.
//!
//! A rule like `10001|10002|10003:14-26 -> 20001|20002|20003:1-13!` says that
//! episodes 14–26 of one anime are episodes 1–13 of another. IDs are given per
//! service: MyAnimeList, Kitsu, AniList. `?` is unknown, `~` repeats the
//! source ID, and a `!` suffix also redirects the destination to itself.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use tracker_core::ServiceId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Redirection {
    pub id: i32,
    pub episodes: (i32, i32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    destination_id: i32,
    source: (i32, i32),
    destination: (i32, i32),
}

#[derive(Debug, Clone, Default)]
pub struct Relations {
    rules: HashMap<i32, Vec<Rule>>,
}

fn column(service: ServiceId) -> usize {
    match service {
        ServiceId::MyAnimeList => 0,
        ServiceId::Kitsu => 1,
        ServiceId::AniList => 2,
    }
}

static RULE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^(?<src_ids>(?:\d+|[?~])(?:\|(?:\d+|[?~]))*):(?<src_start>\d+)(?:-(?<src_end>\d+|\?))? -> ",
        r"(?<dst_ids>(?:\d+|[?~])(?:\|(?:\d+|[?~]))*):(?<dst_start>\d+)(?:-(?<dst_end>\d+|\?))?(?<bang>!)?$",
    ))
    .expect("valid pattern")
});

impl Relations {
    /// Reads the rules for one service's IDs. Malformed rules are skipped.
    pub fn parse(contents: &str, service: ServiceId) -> Self {
        let column = column(service);
        let mut relations = Self::default();
        let mut in_rules = false;
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(section) = line.strip_prefix("::") {
                in_rules = section == "rules";
                continue;
            }
            if !in_rules {
                continue;
            }
            let line = line.strip_prefix('-').map_or(line, str::trim);
            relations.parse_rule(line, column);
        }
        relations
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    fn parse_rule(&mut self, rule: &str, column: usize) {
        let Some(m) = RULE.captures(rule) else {
            return;
        };
        // `None` for `?` and `~`, which mean different things for source and
        // destination.
        let id = |group: &str| {
            m.name(group)?
                .as_str()
                .split('|')
                .nth(column)?
                .parse::<i32>()
                .ok()
        };
        let range = |start: &str, end: &str| {
            let first = m.name(start).map_or(0, |s| s.as_str().parse().unwrap_or(0));
            match m.name(end).map(|e| e.as_str()) {
                None => (first, first),
                // `?` is open-ended.
                Some(end) => (first, end.parse().unwrap_or(i32::MAX)),
            }
        };
        let Some(source_id) = id("src_ids") else {
            return;
        };
        let source = range("src_start", "src_end");
        let destination_id = id("dst_ids").unwrap_or(source_id);
        let destination = range("dst_start", "dst_end");
        self.add(source_id, source, destination_id, destination);
        if m.name("bang").is_some() {
            self.add(destination_id, source, destination_id, destination);
        }
    }

    fn add(&mut self, id: i32, source: (i32, i32), destination_id: i32, destination: (i32, i32)) {
        self.rules.entry(id).or_default().push(Rule {
            destination_id,
            source,
            destination,
        });
    }

    /// Where episode `episode` of anime `id` belongs, if a rule covers it.
    pub fn find(&self, id: i32, episode: i32) -> Option<Redirection> {
        self.rules.get(&id)?.iter().find_map(|rule| {
            let distance = episode - rule.source.0;
            if distance < 0 || episode > rule.source.1 {
                return None;
            }
            let mut destination = rule.destination.0;
            if rule.destination.0 != rule.destination.1 {
                destination += distance;
            }
            (destination <= rule.destination.1).then_some(Redirection {
                id: rule.destination_id,
                episodes: (destination, destination),
            })
        })
    }

    /// Redirects an episode range; both ends must go to the same anime.
    pub fn find_redirection(&self, id: i32, episodes: (i32, i32)) -> Option<Redirection> {
        let first = self.find(id, episodes.0)?;
        if episodes.0 == episodes.1 {
            return Some(first);
        }
        let second = self.find(id, episodes.1)?;
        (first.id == second.id).then_some(Redirection {
            id: first.id,
            episodes: (first.episodes.0, second.episodes.0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# comment
::meta
- version: 1.3.0
- 1|2|3:1 -> 4|5|6:1

::rules
# 3-gatsu no Lion -> ~ 2nd season
- 31646|11380|21366:23-44 -> 35180|13401|98478:1-22
# 100-man no Inochi -> ~ 2nd Season
- 41380|43367|116242:13-24 -> 44881|43883|127366:1-12!
# Open-ended
- 100|?|300:13-? -> ~|?|~:13-?
- 7|8|9:0 -> 10|11|~:1
not a rule
";

    #[test]
    fn redirects_by_service_column() {
        let anilist = Relations::parse(SAMPLE, ServiceId::AniList);
        assert_eq!(
            anilist.find(21366, 25),
            Some(Redirection {
                id: 98478,
                episodes: (3, 3)
            })
        );
        assert_eq!(anilist.find(21366, 22), None);
        assert_eq!(anilist.find(21366, 45), None);

        let mal = Relations::parse(SAMPLE, ServiceId::MyAnimeList);
        assert_eq!(
            mal.find(31646, 44),
            Some(Redirection {
                id: 35180,
                episodes: (22, 22)
            })
        );
        // The `::meta` section is ignored.
        assert_eq!(mal.find(1, 1), None);
    }

    #[test]
    fn bang_redirects_destination_to_itself() {
        let anilist = Relations::parse(SAMPLE, ServiceId::AniList);
        // Fansubs numbering the sequel 13–24 under the sequel's own title.
        assert_eq!(
            anilist.find(127366, 13),
            Some(Redirection {
                id: 127366,
                episodes: (1, 1)
            })
        );
    }

    #[test]
    fn tilde_repeats_source_and_question_mark_is_open() {
        let anilist = Relations::parse(SAMPLE, ServiceId::AniList);
        assert_eq!(
            anilist.find(300, 500),
            Some(Redirection {
                id: 300,
                episodes: (500, 500)
            })
        );
        // `?` source IDs make the rule unusable for that service.
        let kitsu = Relations::parse(SAMPLE, ServiceId::Kitsu);
        assert_eq!(kitsu.find(0, 13), None);
        // Episode 0 to the same anime's episode 1 (`~` destination).
        assert_eq!(
            anilist.find(9, 0),
            Some(Redirection {
                id: 9,
                episodes: (1, 1)
            })
        );
    }

    #[test]
    fn ranges_must_land_on_one_anime() {
        let anilist = Relations::parse(SAMPLE, ServiceId::AniList);
        assert_eq!(
            anilist.find_redirection(21366, (23, 26)),
            Some(Redirection {
                id: 98478,
                episodes: (1, 4)
            })
        );
        assert_eq!(anilist.find_redirection(21366, (20, 26)), None);
    }

    #[test]
    fn bundled_file_parses() {
        let text = include_str!("../../../src-tauri/resources/anime-relations.txt");
        let relations = Relations::parse(text, ServiceId::AniList);
        assert!(relations.rules.len() > 100, "{}", relations.rules.len());
    }
}
