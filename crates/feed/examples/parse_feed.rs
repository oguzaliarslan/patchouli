//! Parses a saved RSS file and prints what each item became:
//! `cargo run -p patchouli-feed --example parse_feed -- feed.xml [url]`.

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .expect("usage: parse_feed <file.xml> [feed url]");
    let url = args.next().unwrap_or_default();
    let text = std::fs::read_to_string(&path).expect("readable file");
    let feed = patchouli_feed::Feed::parse(&url, &text).expect("an RSS feed");
    println!("{}: {} items", feed.title, feed.items.len());
    let with =
        |f: &dyn Fn(&patchouli_feed::FeedItem) -> bool| feed.items.iter().filter(|i| f(i)).count();
    println!(
        "size: {}  seeders: {}  magnet: {}  info link: {}  date: {}  episode: {}  group: {}",
        with(&|i| i.file_size > 0),
        with(&|i| i.seeders.is_some()),
        with(&|i| !i.magnet_link.is_empty()),
        with(&|i| !i.info_link.is_empty()),
        with(&|i| i.pub_time > 0),
        with(&|i| i.parsed.episode_range().is_some()),
        with(&|i| !i.group().is_empty()),
    );
    for item in feed.items.iter().take(3) {
        println!(
            "- {} | {} bytes | seeders {:?} | {} | title {:?} ep {:?} group {:?} res {:?}",
            item.title,
            item.file_size,
            item.seeders,
            item.description,
            item.parsed.title(),
            item.parsed.episode_range(),
            item.group(),
            item.resolution()
        );
    }
}
