//! Prints what the bundled player list detects right now, as JSON.
//!
//! ```text
//! cargo run -p tracker-detect --example detect
//! ```

fn main() {
    let data = include_str!("../../../src-tauri/resources/players.anisthesia");
    let players = tracker_detect::parse_players(data).expect("players data");
    let detections = tracker_detect::detect(&players);
    println!("{}", serde_json::to_string_pretty(&detections).unwrap());
}
