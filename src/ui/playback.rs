//! Playback screen — ani-cli log stream + player controls.

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::state::AppState;

/// Render the playback screen.
pub fn render(frame: &mut Frame, state: &AppState) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header: now playing
            Constraint::Min(0),    // log stream
            Constraint::Length(3), // controls
        ])
        .split(area);

    render_header(frame, chunks[0], state);
    render_logs(frame, chunks[1], state);
    render_controls(frame, chunks[2]);
}

fn render_header(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let title = state
        .now_playing
        .as_deref()
        .unwrap_or("Starting playback...");

    let header = Paragraph::new(Line::from(vec![
        Span::styled("▶ ", Style::default().fg(Color::Rgb(180, 0, 255)).add_modifier(Modifier::BOLD)),
        Span::styled(title, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(60, 60, 80)))
            .style(Style::default().bg(Color::Rgb(10, 10, 16))),
    );
    frame.render_widget(header, area);
}

fn render_logs(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    // Show last N lines that fit in the area
    let visible = area.height.saturating_sub(2) as usize;
    let logs    = &state.playback_logs;
    let start   = logs.len().saturating_sub(visible);

    let items: Vec<ListItem> = logs[start..]
        .iter()
        .map(|line| {
            let style = if line.contains("error") || line.contains("Error") {
                Style::default().fg(Color::Rgb(255, 80, 80))
            } else if line.contains("›") || line.starts_with('[') {
                Style::default().fg(Color::Rgb(180, 0, 255))
            } else {
                Style::default().fg(Color::Rgb(180, 180, 180))
            };
            ListItem::new(Line::from(Span::styled(format!("  {}", line), style)))
        })
        .collect();

    let log_list = List::new(items).block(
        Block::default()
            .title(Span::styled(
                " Output ",
                Style::default().fg(Color::Rgb(160, 160, 160)),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(40, 40, 60)))
            .style(Style::default().bg(Color::Rgb(8, 8, 14))),
    );
    frame.render_widget(log_list, area);
}

fn render_controls(frame: &mut Frame, area: ratatui::layout::Rect) {
    let controls = Paragraph::new(Line::from(vec![
        Span::styled(" q ", Style::default().fg(Color::Black).bg(Color::Rgb(180, 0, 255)).add_modifier(Modifier::BOLD)),
        Span::styled(" Stop    ", Style::default().fg(Color::Rgb(180, 180, 180))),
        Span::styled(" n ", Style::default().fg(Color::Black).bg(Color::Rgb(60, 60, 80)).add_modifier(Modifier::BOLD)),
        Span::styled(" Next Ep    ", Style::default().fg(Color::Rgb(180, 180, 180))),
        Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Rgb(60, 60, 80)).add_modifier(Modifier::BOLD)),
        Span::styled(" Return", Style::default().fg(Color::Rgb(180, 180, 180))),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(40, 40, 60)))
            .style(Style::default().bg(Color::Rgb(10, 10, 16))),
    );
    frame.render_widget(controls, area);
}
