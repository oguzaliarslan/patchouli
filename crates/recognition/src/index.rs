//! Normalised titles to anime IDs (Taiga `track/recognition_cache.cpp`).

use std::collections::HashMap;

use patchouli_core::{Anime, AnimeSettings};

use crate::normalize::normalize;

/// User-defined alternative titles.
pub const WEIGHT_USER: f32 = 2.0;
/// Romaji, English and Japanese titles.
pub const WEIGHT_MAIN: f32 = 1.0;
/// Synonyms, and the romaji title with the year (`Title (2008)`).
pub const WEIGHT_OTHER: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Match {
    pub id: i32,
    pub weight: f32,
}

#[derive(Debug, Clone, Default)]
pub struct TitleIndex {
    titles: HashMap<String, HashMap<i32, f32>>,
}

impl TitleIndex {
    pub fn new<'a>(
        anime: impl IntoIterator<Item = &'a Anime>,
        settings: &HashMap<i32, AnimeSettings>,
    ) -> Self {
        let mut index = Self::default();
        for item in anime {
            index.add(item, settings.get(&item.id));
        }
        index
    }

    pub fn len(&self) -> usize {
        self.titles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.titles.is_empty()
    }

    pub fn add(&mut self, item: &Anime, settings: Option<&AnimeSettings>) {
        let mut add = |title: &str, weight: f32| {
            let normalized = normalize(title);
            if normalized.is_empty() {
                return;
            }
            let matches = self.titles.entry(normalized).or_default();
            let best = matches.entry(item.id).or_insert(weight);
            if *best < weight {
                *best = weight;
            }
        };
        if let Some(settings) = settings {
            for synonym in &settings.synonyms {
                add(synonym, WEIGHT_USER);
            }
        }
        add(&item.titles.romaji, WEIGHT_MAIN);
        add(&item.titles.english, WEIGHT_MAIN);
        add(&item.titles.japanese, WEIGHT_MAIN);
        if item.date_started.year != 0 {
            let year = item.date_started.year.to_string();
            if !item.titles.romaji.contains(&year) {
                add(&format!("{} ({year})", item.titles.romaji), WEIGHT_OTHER);
            }
        }
        for synonym in &item.titles.synonyms {
            add(synonym, WEIGHT_OTHER);
        }
    }

    pub fn remove(&mut self, id: i32) {
        for matches in self.titles.values_mut() {
            matches.remove(&id);
        }
    }

    /// Anime with this normalised title, best match first (ties by ID).
    pub fn find(&self, normalized: &str) -> Vec<Match> {
        let mut matches: Vec<Match> = self
            .titles
            .get(normalized)
            .into_iter()
            .flatten()
            .map(|(&id, &weight)| Match { id, weight })
            .collect();
        matches.sort_by(|a, b| b.weight.total_cmp(&a.weight).then(a.id.cmp(&b.id)));
        matches
    }
}
