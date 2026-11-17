//! Lenient column readers.
//!
//! Taiga binds many numbers as strings (`QString::number`) and its columns are
//! loosely typed, so a value may come back as INTEGER, REAL, TEXT or NULL.
//! Like Qt's `QVariant::toInt`, anything unreadable becomes 0 or empty.

use rusqlite::Row;
use rusqlite::types::ValueRef;

fn parse_text(bytes: &[u8]) -> Option<f64> {
    std::str::from_utf8(bytes).ok()?.trim().parse().ok()
}

pub fn int(row: &Row, column: &str) -> rusqlite::Result<i64> {
    Ok(match row.get_ref(column)? {
        ValueRef::Integer(i) => i,
        ValueRef::Real(f) => f as i64,
        ValueRef::Text(t) => std::str::from_utf8(t)
            .ok()
            .and_then(|s| s.trim().parse::<i64>().ok())
            .or_else(|| parse_text(t).map(|f| f as i64))
            .unwrap_or(0),
        ValueRef::Null | ValueRef::Blob(_) => 0,
    })
}

pub fn int32(row: &Row, column: &str) -> rusqlite::Result<i32> {
    int(row, column).map(|v| v as i32)
}

pub fn real(row: &Row, column: &str) -> rusqlite::Result<f64> {
    Ok(match row.get_ref(column)? {
        ValueRef::Integer(i) => i as f64,
        ValueRef::Real(f) => f,
        ValueRef::Text(t) => parse_text(t).unwrap_or(0.0),
        ValueRef::Null | ValueRef::Blob(_) => 0.0,
    })
}

pub fn boolean(row: &Row, column: &str) -> rusqlite::Result<bool> {
    int(row, column).map(|v| v != 0)
}

pub fn text(row: &Row, column: &str) -> rusqlite::Result<String> {
    Ok(match row.get_ref(column)? {
        ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
        ValueRef::Integer(i) => i.to_string(),
        ValueRef::Real(f) => f.to_string(),
        ValueRef::Null | ValueRef::Blob(_) => String::new(),
    })
}

pub fn optional_text(row: &Row, column: &str) -> rusqlite::Result<Option<String>> {
    Ok(match row.get_ref(column)? {
        ValueRef::Null => None,
        _ => Some(text(row, column)?),
    })
}

/// A JSON array of strings, as Taiga stores lists. Invalid JSON is an empty list.
pub fn string_list(row: &Row, column: &str) -> rusqlite::Result<Vec<String>> {
    Ok(serde_json::from_str(&text(row, column)?).unwrap_or_default())
}

pub fn to_json(list: &[String]) -> String {
    serde_json::to_string(list).unwrap_or_else(|_| "[]".to_owned())
}
