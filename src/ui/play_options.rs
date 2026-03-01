//! Playback options overlay.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::state::AppState;

const QUALITY_LABELS: [&str; 5] = ["best", "1080p", "720p", "480p", "360p"];

/// Render the playback options overlay on top of the detail screen.
pub fn render_overlay(frame: &mut Frame, state: &AppState) {
    let area = centered_rect(56, 58, frame.area());
    frame.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(area);

    let can_dub = state
        .selected_anime
        .as_ref()
        .map(|anime| anime.has_dub())
        .unwrap_or(false);
    let audio_label = if !can_dub {
        "Sub only"
    } else if state.pending_dub {
        "Dub"
    } else {
        "Sub"
    };
    let header = Paragraph::new(vec![
        Line::from(Span::styled(
            "Choose quality before launch",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled(" Audio ", Style::default().fg(Color::Black).bg(Color::White)),
            Span::raw(" "),
            Span::styled(
                audio_label,
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Rgb(60, 0, 100))
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ])
    .block(
        Block::default()
            .title(Span::styled(
                " ▶ Playback Options ",
                Style::default()
                    .fg(Color::Rgb(180, 0, 255))
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(180, 0, 255)))
            .style(Style::default().bg(Color::Rgb(15, 15, 22))),
    );
    frame.render_widget(header, chunks[0]);

    let items: Vec<ListItem> = QUALITY_LABELS
        .iter()
        .enumerate()
        .map(|(i, quality)| {
            let style = if i == state.playback_quality_cursor {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Rgb(60, 0, 100))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Rgb(205, 205, 215))
            };
            let prefix = if i == state.playback_quality_cursor { "▶ " } else { "  " };
            ListItem::new(Line::from(Span::styled(format!("{prefix}{quality}"), style)))
        })
        .collect();

    let mut list_state = ListState::default().with_selected(Some(state.playback_quality_cursor));
    frame.render_stateful_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::LEFT | Borders::RIGHT)
                .border_style(Style::default().fg(Color::Rgb(180, 0, 255)))
                .style(Style::default().bg(Color::Rgb(12, 12, 18))),
        ),
        chunks[1],
        &mut list_state,
    );

    let footer = Paragraph::new(Line::from(vec![
        Span::styled(" Enter ", Style::default().fg(Color::Black).bg(Color::White)),
        Span::raw(" Launch  "),
        Span::styled(" h/l ", Style::default().fg(Color::Black).bg(Color::Rgb(180, 0, 255))),
        Span::raw(if can_dub { " Toggle audio  " } else { " Audio locked  " }),
        Span::styled(" j/k ", Style::default().fg(Color::Black).bg(Color::Rgb(180, 0, 255))),
        Span::raw(" Quality"),
    ]))
    .block(
        Block::default()
            .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
            .border_style(Style::default().fg(Color::Rgb(180, 0, 255)))
            .style(Style::default().bg(Color::Rgb(12, 12, 18))),
    );
    frame.render_widget(footer, chunks[2]);
}

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
