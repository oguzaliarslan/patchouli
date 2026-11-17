//! Compares the port with output recorded from the C++ Anitomy.
//!
//! `data/golden.jsonl` holds one object per line: an input and the elements the
//! C++ library returned for it (`[kind, value, position]`). The inputs are
//! Anitomy's own test cases (`test/data.json`) plus extra edge cases. The C++
//! sorts elements with an unstable sort, so elements at the same position are
//! compared in a fixed order.

use serde_json::Value;

type Row = (String, String, usize);

fn normalize(mut rows: Vec<Row>) -> Vec<Row> {
    rows.sort_by(|a, b| (a.2, &a.0, &a.1).cmp(&(b.2, &b.0, &b.1)));
    rows
}

#[test]
fn matches_cpp_anitomy() {
    let data = include_str!("data/golden.jsonl");
    let mut failures = Vec::new();
    let mut count = 0;
    for line in data.lines().filter(|l| !l.is_empty()) {
        let case: Value = serde_json::from_str(line).unwrap();
        let input = case["input"].as_str().unwrap();
        let expected: Vec<Row> = case["elements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e[0].as_str().unwrap().to_owned(),
                    e[1].as_str().unwrap().to_owned(),
                    e[2].as_u64().unwrap() as usize,
                )
            })
            .collect();
        let actual: Vec<Row> = patchouli_anitomy::parse(input)
            .into_iter()
            .map(|e| (e.kind.as_str().to_owned(), e.value, e.position))
            .collect();
        count += 1;
        let (expected, actual) = (normalize(expected), normalize(actual));
        if expected != actual {
            failures.push(format!(
                "{input}\n  expected: {expected:?}\n  actual:   {actual:?}"
            ));
        }
    }
    assert!(count > 250, "only {count} cases");
    assert!(
        failures.is_empty(),
        "{} of {count} cases differ:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
