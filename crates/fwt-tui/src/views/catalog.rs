use fwt_app::state::CatalogState;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::theme::Theme;

const DESIGN_SYSTEMS: [&str; 2] = ["Cupertino", "Material"];

pub fn render_catalog_view(frame: &mut Frame, area: Rect, state: &CatalogState, theme: &Theme) {
    if state.categories.is_empty() {
        let msg = Paragraph::new("No categories available.")
            .style(Style::default().fg(theme.muted_text))
            .centered();
        frame.render_widget(msg, area);
        return;
    }

    let (design_systems, base): (Vec<_>, Vec<_>) = state
        .categories
        .iter()
        .partition(|c| DESIGN_SYSTEMS.contains(&c.name.as_str()));

    let [ds_area, base_area] = Layout::vertical([
        Constraint::Length(design_systems.len() as u16 / 2 + 3),
        Constraint::Min(0),
    ])
    .areas(area);

    render_category_section(frame, ds_area, "design systems", &design_systems, theme);
    render_category_section(frame, base_area, "base widgets", &base, theme);
}

fn render_category_section(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    categories: &[&fwt_domain::widget::CategorySummary],
    theme: &Theme,
) {
    let block = Block::default()
        .title(format!("┌─ {title} "))
        .borders(Borders::TOP)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(theme.accent));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if categories.is_empty() {
        return;
    }

    let cols = 3.max(1);
    let rows = categories.len().div_ceil(cols);
    let row_constraints: Vec<Constraint> = (0..rows).map(|_| Constraint::Length(1)).collect();
    let row_areas = Layout::vertical(row_constraints).split(inner);

    for (row_idx, row_area) in row_areas.iter().enumerate() {
        let col_constraints: Vec<Constraint> = (0..cols)
            .map(|_| Constraint::Ratio(1, cols as u32))
            .collect();
        let col_areas = Layout::horizontal(col_constraints).split(*row_area);
        for (col_idx, col_area) in col_areas.iter().enumerate() {
            if let Some(cat) = categories.get(row_idx * cols + col_idx) {
                let label = format!("{} · {} widgets", cat.name, cat.widget_count);
                frame.render_widget(
                    Paragraph::new(label).style(Style::default().fg(theme.text)),
                    *col_area,
                );
            }
        }
    }
}
