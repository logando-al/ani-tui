//! Help overlay — keybinding reference, shown with ? key.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

/// Render the help overlay centered on screen.
pub fn render_overlay(frame: &mut Frame) {
    let area = centered_rect(55, 70, frame.area());
    frame.render_widget(Clear, area);

    let keybindings = vec![
        ("Navigation", vec![
            ("j / ↓",      "Move down (rows / menus)"),
            ("k / ↑",      "Move up (rows / menus)"),
            ("h / ←",      "Scroll left (cards / episodes / related)"),
            ("l / →",      "Scroll right (cards / episodes / related)"),
            ("Tab",        "Toggle Detail focus: Episodes / More Like This"),
        ]),
        ("Actions", vec![
            ("Enter",      "Open detail (Home) / start or continue (Detail)"),
            ("d",          "Open detail from Home"),
            ("r",          "Resume from Home / next episode"),
            ("Esc",        "Back / close (restores prior related detail)"),
            ("/",          "Open search"),
            ("+",          "Add to / remove from watchlist"),
            ("n",          "Play next episode from Detail"),
            ("q",          "Quit / Stop playback"),
        ]),
        ("Other", vec![
            ("?",          "Toggle this help"),
            ("Shift+R",    "Refresh home screen"),
            ("Search",     "Type normally, use ↑/↓ to pick results and preview"),
            ("Ctrl+C",     "Force quit"),
        ]),
    ];

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            " ani-tui — Keybindings ",
            Style::default()
                .fg(Color::Rgb(180, 0, 255))
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    for (section, bindings) in &keybindings {
        lines.push(Line::from(Span::styled(
            format!("  {} ", section),
            Style::default()
                .fg(Color::Rgb(220, 220, 220))
                .add_modifier(Modifier::UNDERLINED),
        )));
        for (key, desc) in bindings {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {:>12}  ", key),
                    Style::default()
                        .fg(Color::Rgb(180, 0, 255))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(*desc, Style::default().fg(Color::Rgb(190, 190, 190))),
            ]));
        }
        lines.push(Line::from(""));
    }

    lines.push(Line::from(Span::styled(
        "  Press ? or Esc to close",
        Style::default().fg(Color::Rgb(100, 100, 120)),
    )));

    let help = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(180, 0, 255)))
            .style(Style::default().bg(Color::Rgb(12, 12, 20))),
    );
    frame.render_widget(help, area);
}

/// Render the toast notification bar at the bottom of the screen.
pub fn render_toast(frame: &mut Frame, message: &str) {
    let area   = frame.area();
    let height = 1u16;
    let toast_area = Rect {
        x:      area.x,
        y:      area.y + area.height.saturating_sub(height),
        width:  area.width,
        height,
    };

    let toast = Paragraph::new(Span::styled(
        format!("  ✓ {}", message),
        Style::default()
            .fg(Color::Black)
            .bg(Color::Rgb(180, 0, 255))
            .add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(toast, toast_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let vert = Layout::default()
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
        .split(vert[1])[1]
}
