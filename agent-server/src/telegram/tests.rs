//! Tests for the Telegram adapter's event-handling behavior.

use super::formatting::truncate;

#[test]
fn truncates_long_text() {
    let text = "x".repeat(4000);
    assert!(truncate(&text).ends_with("[Response truncated]"));
}

#[test]
fn truncation_preserves_short_text() {
    assert_eq!(truncate("hello"), "hello");
}
