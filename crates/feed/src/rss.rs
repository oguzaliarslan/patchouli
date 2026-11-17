//! RSS 2.0 documents (Taiga v1 `base/rss.cpp`).

use std::collections::BTreeMap;

use patchouli_compat::xml::{Element, parse, remove_invalid_character_references};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Channel {
    pub title: String,
    pub link: String,
    pub description: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Item {
    pub title: String,
    pub link: String,
    pub description: String,
    pub category: String,
    pub guid: String,
    /// `isPermaLink`, true unless the feed says otherwise.
    pub guid_is_permalink: bool,
    pub pub_date: String,
    pub enclosure_url: String,
    pub enclosure_length: String,
    pub enclosure_type: String,
    /// Elements with a namespace prefix, e.g. `nyaa:seeders`.
    pub namespace_elements: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    pub channel: Channel,
    pub items: Vec<Item>,
}

pub fn parse_document(text: &str) -> Result<Document, String> {
    let text = remove_invalid_character_references(text);
    let roots = parse(&text)?;
    let channel = roots
        .iter()
        .find(|e| e.name == "rss")
        .and_then(|rss| rss.child("channel"))
        .ok_or("Not an RSS feed.")?;
    Ok(Document {
        channel: Channel {
            title: channel.text_of("title").to_owned(),
            link: channel.text_of("link").to_owned(),
            description: channel.text_of("description").to_owned(),
        },
        items: channel.children_named("item").map(parse_item).collect(),
    })
}

fn parse_item(node: &Element) -> Item {
    let guid = node.child("guid");
    let enclosure = node.child("enclosure");
    let attr = |name| enclosure.map_or(String::new(), |e| e.attr(name).to_owned());
    Item {
        title: node.text_of("title").to_owned(),
        link: node.text_of("link").to_owned(),
        description: node.text_of("description").to_owned(),
        category: node.text_of("category").to_owned(),
        guid: guid.map_or(String::new(), |g| g.text.clone()),
        guid_is_permalink: guid.is_none_or(|g| g.attr("isPermaLink") != "false"),
        pub_date: node.text_of("pubDate").to_owned(),
        enclosure_url: attr("url"),
        enclosure_length: attr("length"),
        enclosure_type: attr("type"),
        namespace_elements: node
            .children
            .iter()
            .filter(|c| c.name.contains(':'))
            .map(|c| (c.name.clone(), c.text.clone()))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_items_with_namespaces() {
        let doc = parse_document(
            r#"<?xml version="1.0"?>
<rss version="2.0" xmlns:nyaa="https://nyaa.si/xmlns/nyaa">
  <channel>
    <title>Nyaa</title><link>https://nyaa.si/</link>
    <item>
      <title>[Group] Show - 05 [1080p].mkv</title>
      <link>https://nyaa.si/download/1.torrent</link>
      <guid isPermaLink="true">https://nyaa.si/view/1</guid>
      <pubDate>Sat, 01 Jun 2024 12:00:00 -0000</pubDate>
      <nyaa:seeders>12</nyaa:seeders><nyaa:size>1.4 GiB</nyaa:size>
      <description><![CDATA[<a href="https://nyaa.si/view/1">#1</a>]]></description>
    </item>
  </channel>
</rss>"#,
        )
        .unwrap();
        assert_eq!(doc.channel.link, "https://nyaa.si/");
        let item = &doc.items[0];
        assert_eq!(item.guid, "https://nyaa.si/view/1");
        assert!(item.guid_is_permalink);
        assert_eq!(item.namespace_elements["nyaa:seeders"], "12");
        assert!(item.description.starts_with("<a href"));
        assert!(parse_document("<html/>").is_err());
    }
}
