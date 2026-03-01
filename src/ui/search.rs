//! Search overlay — floats centered over the current screen.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::state::AppState;

/// Render the search overlay on top of whatever screen is below.
pub fn render_overlay(frame: &mut Frame, state: &AppState) {
    let area    = centered_rect(60, 50, frame.area());

    // Clear the background area so it's not transparent
    frame.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // input box
            Constraint::Min(0),    // results list
        ])
        .split(area);

    render_input(frame, chunks[0], state);
    render_results(frame, chunks[1], state);
}

/// The search input box.
fn render_input(frame: &mut Frame, area: Rect, state: &AppState) {
    let query_display = format!("{}_", state.search_query); // cursor indicator

    let input = Paragraph::new(Span::styled(
        &query_display,
        Style::default().fg(Color::White),
    ))
    .block(
        Block::default()
            .title(Span::styled(
                " 🔍 Search Anime ",
                Style::default()
                    .fg(Color::Rgb(180, 0, 255))
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(180, 0, 255)))
            .style(Style::default().bg(Color::Rgb(15, 15, 22))),
    );
    frame.render_widget(input, area);
}

/// The results list below the input.
fn render_results(frame: &mut Frame, area: Rect, state: &AppState) {
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_style(Style::default().fg(Color::Rgb(180, 0, 255)))
        .style(Style::default().bg(Color::Rgb(12, 12, 18)));

    if state.search_results.is_empty() {
        let msg = if state.search_query.is_empty() {
            "Type to search anime..."
        } else {
            "No results found."
        };
        let para = Paragraph::new(Span::styled(
            msg,
            Style::default().fg(Color::Rgb(100, 100, 120)),
        ))
        .block(block);
        frame.render_widget(para, area);
        return;
    }

    let items: Vec<ListItem> = state
        .search_results
        .iter()
        .enumerate()
        .map(|(i, anime)| {
            let score = anime
                .score
                .map(|s| format!("{:.1}", s as f32 / 10.0))
                .unwrap_or_else(|| "N/A".to_string());
            let eps = anime
                .episodes
                .map(|e| format!("{} eps", e))
                .unwrap_or_else(|| "? eps".to_string());

            let style = if i == state.search_cursor {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Rgb(60, 0, 100))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Rgb(200, 200, 200))
            };

            let prefix = if i == state.search_cursor { "▶ " } else { "  " };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{}{}", prefix, anime.display_title()), style),
                Span::styled(
                    format!("  ★{}  {}", score, eps),
                    Style::default().fg(Color::Rgb(120, 120, 140)),
                ),
            ]))
        })
        .collect();

    let mut list_state = ListState::default().with_selected(Some(state.search_cursor));
    frame.render_stateful_widget(
        List::new(items).block(block),
        area,
        &mut list_state,
    );
}

/// Returns a Rect centered on `r` with given width% and height%.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
