//! A small element tree over quick-xml, enough for Taiga v1's files.

use std::collections::HashMap;

use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Element {
    pub name: String,
    pub attributes: HashMap<String, String>,
    pub text: String,
    pub children: Vec<Element>,
}

impl Element {
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children.iter().filter(move |c| c.name == name)
    }

    /// Text of a child element, or "".
    pub fn text_of(&self, name: &str) -> &str {
        self.child(name).map_or("", |c| c.text.as_str())
    }

    pub fn attr(&self, name: &str) -> &str {
        self.attributes.get(name).map_or("", String::as_str)
    }

    pub fn has_attr(&self, name: &str) -> bool {
        self.attributes.contains_key(name)
    }

    pub fn attr_bool(&self, name: &str) -> bool {
        self.attr(name) == "true"
    }

    pub fn attr_int(&self, name: &str) -> i64 {
        self.attr(name).trim().parse().unwrap_or(0)
    }
}

fn element(start: &BytesStart) -> Element {
    let name = start.name().as_ref().to_owned();
    let attributes = start
        .attributes()
        .flatten()
        .filter_map(|a| {
            let key = a.key.as_ref().to_owned();
            let value = a
                .normalized_value(XmlVersion::Implicit1_0)
                .ok()?
                .into_owned();
            Some((key, value))
        })
        .collect();
    Element {
        name,
        attributes,
        ..Element::default()
    }
}

/// Parses a document into its top-level elements. Taiga v1 wrote two roots
/// (`<meta>` and the content), which is why this returns a list.
pub fn parse(text: &str) -> Result<Vec<Element>, String> {
    let mut reader = Reader::from_str(text);
    let mut stack: Vec<Element> = Vec::new();
    let mut roots = Vec::new();
    loop {
        let event = reader
            .read_event()
            .map_err(|e| format!("XML error at {}: {e}", reader.buffer_position()))?;
        match event {
            Event::Start(start) => stack.push(element(&start)),
            Event::Empty(start) => {
                let node = element(&start);
                match stack.last_mut() {
                    Some(parent) => parent.children.push(node),
                    None => roots.push(node),
                }
            }
            Event::End(_) => {
                let Some(mut node) = stack.pop() else {
                    return Err("unbalanced end tag".into());
                };
                // Whitespace between child elements, and around values.
                let trimmed = node.text.trim();
                if trimmed.len() != node.text.len() {
                    node.text = trimmed.to_owned();
                }
                match stack.last_mut() {
                    Some(parent) => parent.children.push(node),
                    None => roots.push(node),
                }
            }
            Event::Text(text) => {
                if let Some(node) = stack.last_mut() {
                    node.text
                        .push_str(&text.xml_content(XmlVersion::Implicit1_0));
                }
            }
            // Entities arrive as separate events (`&amp;`, `&#38;`).
            Event::GeneralRef(reference) => {
                if let Some(node) = stack.last_mut() {
                    node.text.push_str(&resolve_reference(&reference));
                }
            }
            Event::CData(data) => {
                if let Some(node) = stack.last_mut() {
                    node.text
                        .push_str(&data.xml_content(XmlVersion::Implicit1_0));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err("unclosed elements".into());
    }
    Ok(roots)
}

fn resolve_reference(reference: &BytesRef) -> String {
    if reference.is_char_ref() {
        if let Ok(Some(ch)) = reference.resolve_char_ref() {
            return ch.to_string();
        }
    }
    let name = reference.clone().into_inner();
    match name.as_ref() {
        "amp" => "&".into(),
        "lt" => "<".into(),
        "gt" => ">".into(),
        "quot" => "\"".into(),
        "apos" => "'".into(),
        other => format!("&{other};"),
    }
}

/// Removes numeric references to control characters that XML 1.0 forbids
/// (e.g. `&#03;`, mIRC's colour code), which Taiga v1 sometimes wrote.
pub fn remove_invalid_character_references(text: &str) -> String {
    static PATTERN: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"&#(x?)([0-9A-Fa-f]+);").expect("valid pattern")
    });
    PATTERN
        .replace_all(text, |caps: &regex::Captures| {
            let radix = if caps[1].is_empty() { 10 } else { 16 };
            match u32::from_str_radix(&caps[2], radix) {
                Ok(code) if code <= 0x1F && !matches!(code, 0x9 | 0xA | 0xD) => String::new(),
                _ => caps[0].to_owned(),
            }
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_roots_attributes_and_entities() {
        let roots = parse(
            "<?xml version=\"1.0\"?><meta><version>1.4</version></meta>\
             <database><anime><id name=\"anilist\">21</id><title>Tom &amp; Jerry</title><empty/></anime></database>",
        )
        .unwrap();
        assert_eq!(roots.len(), 2);
        let anime = roots[1].child("anime").unwrap();
        assert_eq!(anime.child("id").unwrap().attr("name"), "anilist");
        assert_eq!(anime.text_of("id"), "21");
        assert_eq!(anime.text_of("title"), "Tom & Jerry");
        assert!(anime.child("empty").is_some());
    }

    #[test]
    fn strips_control_character_references() {
        assert_eq!(
            remove_invalid_character_references("a&#03;b&#x1F;c&#10;d&#65;"),
            "abc&#10;d&#65;"
        );
    }
}
