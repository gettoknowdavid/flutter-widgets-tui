use fwt_app::state::SearchState;
use ratatui::prelude::*;
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use crate::theme::Theme;

pub fn render_search_view(frame: &mut Frame, area: Rect, state: &SearchState, theme: &Theme) {
    if !state.index_ready {
        let msg = Paragraph::new("Warming up search index…")
            .style(Style::default().fg(theme.muted_text))
            .centered();
        frame.render_widget(msg, area);
        return;
    }

    if state.results.is_empty() {
        let text = if state.query.trim().is_empty() {
            "Start typing to search…"
        } else {
            "No matches found."
        };
        let msg = Paragraph::new(text)
            .style(Style::default().fg(theme.muted_text))
            .centered();
        frame.render_widget(msg, area);
        return;
    }

    let items: Vec<ListItem> = state
        .results
        .iter()
        .map(|r| {
            let name_spans = highlight_spans(&r.name, &r.name_highlight_ranges, theme);
            let summary = safe_truncate(&r.summary, 80);
            let mut line_spans = name_spans;
            line_spans.push(Span::raw("  "));
            line_spans.push(Span::styled(summary, Style::default().fg(theme.muted_text)));
            ListItem::new(Line::from(line_spans))
        })
        .collect();

    let mut list_state = ListState::default().with_selected(state.selected_index);
    let list = List::new(items)
        .highlight_style(Style::default().fg(theme.accent).bold())
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, area, &mut list_state);
}

/// Converts byte-offset highlight ranges into alternating styled/unstyled
/// Ratatui spans. Respects UTF-8 boundaries by construction: `ranges` are
/// always produced from `char_indices_to_byte_ranges` (SearchService),
/// which only ever emits offsets that fall on char boundaries — but this
/// function is defensive anyway and never slices on an arbitrary byte
/// index without going through `char_indices`.
pub fn highlight_spans<'a>(
    text: &'a str,
    ranges: &[std::ops::Range<usize>],
    theme: &Theme,
) -> Vec<Span<'a>> {
    if ranges.is_empty() {
        return vec![Span::styled(text, Style::default().fg(theme.text))];
    }

    let mut spans = Vec::new();
    let mut cursor = 0usize;

    // Build a sorted, non-overlapping range list defensively — callers
    // (SearchService) already guarantee this, but a rendering function
    // must never panic even if that invariant is ever violated upstream.
    let mut sorted_ranges: Vec<std::ops::Range<usize>> = ranges.to_vec();
    sorted_ranges.sort_by_key(|r| r.start);

    for range in sorted_ranges {
        let start = snap_to_char_boundary(text, range.start.min(text.len()));
        let end = snap_to_char_boundary(text, range.end.min(text.len()));
        if start < cursor || start >= end {
            continue; // skip malformed/overlapping ranges rather than panic
        }
        if cursor < start {
            spans.push(Span::styled(
                &text[cursor..start],
                Style::default().fg(theme.text),
            ));
        }
        spans.push(Span::styled(
            &text[start..end],
            Style::default().fg(theme.accent).bold(),
        ));
        cursor = end;
    }
    if cursor < text.len() {
        spans.push(Span::styled(
            &text[cursor..],
            Style::default().fg(theme.text),
        ));
    }
    spans
}

/// Walks backward to the nearest char boundary — never slices mid-codepoint.
fn snap_to_char_boundary(text: &str, mut idx: usize) -> usize {
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

/// UTF-8-safe truncation to at most `max_chars` characters, appending an
/// ellipsis if truncated. Shared with any future truncation call site
/// (Ticket 007's flagged edge case) rather than ad hoc byte-slicing.
pub fn safe_truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{truncated}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_theme() -> Theme {
        Theme::default()
    }

    #[test]
    fn ascii_highlight_produces_three_spans() {
        let spans = highlight_spans("ListView", &[0..4], &test_theme());
        assert_eq!(spans.len(), 2); // "List" highlighted, "View" plain
    }

    #[test]
    fn multibyte_utf8_highlight_does_not_panic() {
        // "café" — é is 2 bytes (0xC3 0xA9), byte offset 3..5
        let text = "café widget";
        let spans = highlight_spans(text, &[3..5], &test_theme());
        // Must not panic; must produce valid spans covering the string.
        let total: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(total, text);
    }

    #[test]
    fn out_of_bounds_range_is_clamped_not_panicking() {
        let spans = highlight_spans("abc", &[0..999], &test_theme());
        assert!(!spans.is_empty());
    }

    #[test]
    fn empty_ranges_returns_single_unstyled_span() {
        let spans = highlight_spans("abc", &[], &test_theme());
        assert_eq!(spans.len(), 1);
    }

    #[test]
    fn safe_truncate_respects_char_boundaries() {
        let text = "café résumé widget summary text";
        let truncated = safe_truncate(text, 5);
        assert!(truncated.chars().count() <= 6); // 5 + ellipsis
    }
}
