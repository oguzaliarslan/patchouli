//! An episode parsed from a file name (Taiga `track/episode.cpp`).

use patchouli_anitomy::{Element, ElementKind};
use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Episode {
    pub elements: Vec<Element>,
}

/// Qt's `QString::toInt`: the whole string must be a number, else 0.
pub(crate) fn to_int(value: &str) -> i32 {
    value.trim().parse().unwrap_or(0)
}

impl Episode {
    pub fn parse(input: &str) -> Self {
        Self {
            elements: patchouli_anitomy::parse(input),
        }
    }

    pub fn contains(&self, kind: ElementKind) -> bool {
        self.elements.iter().any(|e| e.kind == kind)
    }

    /// The first value of a kind.
    pub fn element(&self, kind: ElementKind) -> Option<&str> {
        self.elements
            .iter()
            .find(|e| e.kind == kind)
            .map(|e| e.value.as_str())
    }

    pub fn values(&self, kind: ElementKind) -> impl Iterator<Item = &str> {
        self.elements
            .iter()
            .filter(move |e| e.kind == kind)
            .map(|e| e.value.as_str())
    }

    pub fn title(&self) -> &str {
        self.element(ElementKind::Title).unwrap_or_default()
    }

    /// First and last episode number. Non-integers (`4a`, `7.5`) count as 0,
    /// as in Taiga.
    pub fn episode_range(&self) -> Option<(i32, i32)> {
        let first = self.element(ElementKind::Episode)?;
        let last = self.values(ElementKind::Episode).last().unwrap_or(first);
        Some((to_int(first), to_int(last)))
    }

    pub fn set_episode_range(&mut self, range: (i32, i32)) {
        self.remove(ElementKind::Episode);
        self.add(ElementKind::Episode, range.0.to_string());
        if range.1 > range.0 {
            self.add(ElementKind::Episode, range.1.to_string());
        }
    }

    pub fn add(&mut self, kind: ElementKind, value: impl Into<String>) {
        self.elements.push(Element {
            kind,
            value: value.into(),
            position: usize::MAX,
        });
    }

    pub fn remove(&mut self, kind: ElementKind) {
        self.elements.retain(|e| e.kind != kind);
    }

    /// Anitomy only reports extensions of video containers.
    pub fn is_video_file(&self) -> bool {
        self.contains(ElementKind::FileExtension)
    }
}
