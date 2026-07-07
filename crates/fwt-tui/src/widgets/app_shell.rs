use crate::theme::Theme;
use fwt_app::state::AppState;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

const TAB_LABELS: [&str; 4] = ["[1] Catalog", "[2] Search", "[3] Favorites", "[4] AI Chat"];
const STATUS_LEGEND: &str = "tab: switch pane · /: search · enter: select · esc: back · q: quit";

/// Renders the outer chrome and returns the inner content `Rect` for the
/// caller (or a dispatched Epic 2+ view function) to render into.
///
/// `AppShell` owns tab bar + status bar; it does NOT render tab content —
/// per TRD 8.3, this is the seam every future view renders through.
pub fn render_app_shell(frame: &mut Frame, area: Rect, _state: &AppState, theme: &Theme) -> Rect {
    // GUARD against degenerate sizes BEFORE calling Layout::split — some
    // constraint solvers can behave oddly at (0,0)/near-zero areas.
    // Ratatui's Layout is generally panic-safe here, but bail early and
    // explicitly rather than relying on that implicitly (ticket criterion 5).
    if area.width == 0 || area.height == 0 {
        // Return area as there is nothing to render here
        return area;
    }

    // Three vertical regions: tab bar (1 row) / content (rest) / status (1 row)
    let [tab_bar_area, content_area, status_bar_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);

    render_tab_bar(frame, tab_bar_area, theme);
    let inner_content = render_content_pane(frame, content_area, theme);
    render_status_bar(frame, status_bar_area, theme);
    inner_content
}

fn render_tab_bar(frame: &mut Frame, area: Rect, theme: &Theme) {
    let mut spans = Vec::with_capacity(TAB_LABELS.len() * 2);
    for (i, label) in TAB_LABELS.iter().enumerate() {
        let style = if i == 0 {
            Style::default().fg(theme.text).underlined()
        } else {
            Style::default().fg(theme.muted_text)
        };
        spans.push(Span::styled(*label, style));
        spans.push(Span::raw("  "));
    }
    frame.render_widget(Line::from(spans), area)
}
fn render_content_pane(frame: &mut Frame, area: Rect, theme: &Theme) -> Rect {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let placeholder = Paragraph::new("Epic 2+ content renders here")
        .style(Style::default().fg(theme.muted_text))
        .centered();
    frame.render_widget(placeholder, inner);

    inner
}
fn render_status_bar(frame: &mut Frame, area: Rect, theme: &Theme) {
    let status = Paragraph::new(STATUS_LEGEND)
        .style(Style::default().fg(theme.muted_text).bg(theme.surface));
    frame.render_widget(status, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn render_to_backend(width: u16, height: u16) -> TestBackend {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = AppState::default();
        let theme = Theme::default();

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_app_shell(frame, area, &state, &theme);
            })
            .unwrap();

        terminal.backend().clone() // TestBackend implements Clone; buffer() readable after
    }

    #[test]
    fn snapshot_default_100x30() {
        let backend = render_to_backend(100, 30);
        // insta asserts the Debug-formatted buffer against a committed .snap
        // baseline — first run creates it; review via `cargo insta review`.
        insta::assert_debug_snapshot!(backend.buffer());
    }

    #[test]
    fn snapshot_reflow_60x20() {
        let backend = render_to_backend(60, 20);
        // Proves Constraint-based layout reflows sanely at a smaller size —
        // NOT asserting pixel-identical content to the 100x30 baseline,
        // just that structure (3 regions, bordered content pane) holds.
        insta::assert_debug_snapshot!(backend.buffer());
    }

    #[test]
    fn pathological_5x3_does_not_panic() {
        // Criterion 5: panic-safety only, NOT visual correctness.
        // The #[test] harness itself is the assertion here — if
        // render_to_backend() panics, this test fails; no manual
        // assert needed beyond "it returned."
        let _backend = render_to_backend(5, 3);
    }

    #[test]
    fn zero_area_does_not_panic() {
        // Extra edge case beyond the ticket's explicit 5x3 ask — cheap
        // insurance against the (0,0) case Ticket 004 flagged as a known
        // hand-off point for this ticket to address.
        let _backend = render_to_backend(0, 0);
    }
}
