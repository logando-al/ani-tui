//! Detail screen — full anime info + scrollable episode list.

use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

use crate::{db::cache::Anime, state::AppState, ui::components::cover::HalfblockCover};

/// Render the detail screen.
pub fn render(frame: &mut Frame, state: &mut AppState) {
    let Some(anime) = state.selected_anime.clone() else {
        return;
    };

    let area = frame.area();

    // Back hint
    let hint = Paragraph::new(Line::from(vec![
        Span::styled(" ← Esc", Style::default().fg(Color::Rgb(120, 120, 120))),
        Span::raw("  "),
        Span::styled("Enter", Style::default().fg(Color::Rgb(180, 0, 255))),
        Span::raw(" Play  "),
        Span::styled("+", Style::default().fg(Color::Rgb(180, 0, 255))),
        Span::raw(" Watchlist  "),
        Span::styled("/", Style::default().fg(Color::Rgb(180, 0, 255))),
        Span::raw(" Search  "),
        Span::styled("?", Style::default().fg(Color::Rgb(180, 0, 255))),
        Span::raw(" Help"),
    ]))
    .style(Style::default().bg(Color::Rgb(10, 10, 16)));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // hint bar
            Constraint::Length(14), // top info panel
            Constraint::Min(0),     // episode list
        ])
        .split(area);

    frame.render_widget(hint, chunks[0]);
    render_info(frame, chunks[1], state, &anime);
    render_episodes(frame, chunks[2], state, &anime);
}

/// Top section: cover + metadata side by side.
fn render_info(frame: &mut Frame, area: Rect, state: &mut AppState, anime: &Anime) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(20), // cover art
            Constraint::Min(0),     // metadata
        ])
        .split(area);

    let cover_frame = if cols[0].width > 4 && cols[0].height > 4 {
        cols[0].inner(Margin { horizontal: 1, vertical: 1 })
    } else {
        cols[0]
    };
    let cover_bg = Block::default().style(Style::default().bg(Color::Rgb(14, 14, 22)));
    frame.render_widget(cover_bg, cover_frame);
    let cover_inner = if cover_frame.width > 2 && cover_frame.height > 2 {
        cover_frame.inner(Margin { horizontal: 1, vertical: 1 })
    } else {
        cover_frame
    };

    // Cover: real image when terminal supports it, halfblock otherwise
    if state.has_image_support() && state.cover_state.is_some() {
        if let Some(ref mut cover) = state.cover_state {
            let image_widget = ratatui_image::StatefulImage::new(None)
                .resize(ratatui_image::Resize::Fit(None));
            frame.render_stateful_widget(image_widget, cover_inner, cover);
        }
    } else {
        frame.render_widget(
            HalfblockCover { anime_id: anime.id, title: anime.display_title() },
            cover_inner,
        );
    }
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(60, 60, 80))),
        cover_frame,
    );

    // Metadata
    render_metadata(frame, cols[1], anime);
}

fn render_metadata(frame: &mut Frame, area: Rect, anime: &Anime) {
    let title   = anime.display_title();
    let score   = anime.score.map(|s| format!("★ {:.1}", s as f32 / 10.0)).unwrap_or_else(|| "★ N/A".to_string());
    let eps     = anime.episodes.map(|e| format!("{} eps", e)).unwrap_or_else(|| "? eps".to_string());
    let fmt     = anime.format.as_deref().unwrap_or("TV");
    let status  = anime.status.as_deref().unwrap_or("Unknown");
    let year    = anime.season_year.map(|y| y.to_string()).unwrap_or_else(|| "?".to_string());
    let season  = anime.season.as_deref().unwrap_or("");
    let genres  = anime.genre_list().join(" · ");
    let desc    = anime
        .description
        .as_deref()
        .unwrap_or("No description available.")
        .chars()
        .take(300)
        .collect::<String>();
    let dub_tag = if anime.has_dub() { "  Sub + Dub" } else { "  Sub only" };

    let lines = vec![
        Line::from(Span::styled(
            title,
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("{}  |  {}  |  {}  |  {} {}  |  {}{}", score, eps, fmt, season, year, status, dub_tag),
            Style::default().fg(Color::Rgb(160, 160, 160)),
        )),
        Line::from(Span::styled(genres, Style::default().fg(Color::Rgb(180, 0, 255)))),
        Line::from(""),
        Line::from(Span::styled(desc, Style::default().fg(Color::Rgb(210, 210, 210)))),
    ];

    let para = Paragraph::new(lines)
        .block(Block::default().style(Style::default().bg(Color::Rgb(10, 10, 16))))
        .wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(para, area);
}

/// Episode list section — horizontal scrolling pills.
fn render_episodes(frame: &mut Frame, area: Rect, state: &AppState, _anime: &Anime) {
    let block = Block::default()
        .title(Span::styled(
            " Episodes ",
            Style::default().fg(Color::Rgb(220, 220, 220)).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::Rgb(60, 60, 80)))
        .style(Style::default().bg(Color::Rgb(10, 10, 16)));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.episode_list.is_empty() {
        let msg = Paragraph::new("No episode data available.")
            .style(Style::default().fg(Color::Rgb(120, 120, 120)));
        frame.render_widget(msg, inner);
        return;
    }

    // Calculate how many pills fit per row
    let pill_width: u16 = 6; // " E99 "
    let pills_per_row   = (inner.width / pill_width).max(1) as usize;
    let selected_ep     = state.selected_episode.unwrap_or(1);

    // Render rows of episode pills
    let rows_needed = (state.episode_list.len() + pills_per_row - 1) / pills_per_row;
    let visible_rows = inner.height.saturating_sub(1) as usize;
    let offset_rows  = state.episode_offset / pills_per_row;

    let mut y = inner.y;
    for row_idx in offset_rows..(offset_rows + visible_rows).min(rows_needed) {
        if y >= inner.y + inner.height {
            break;
        }
        let start   = row_idx * pills_per_row;
        let end     = (start + pills_per_row).min(state.episode_list.len());
        let mut x   = inner.x;

        for &ep in &state.episode_list[start..end] {
            if x + pill_width > inner.x + inner.width {
                break;
            }
            let is_selected = ep == selected_ep;
            let is_watched  = state.watched_episodes.contains(&ep);
            let label       = format!(" E{:<3}", ep);
            let style       = if is_selected {
                // Purple highlight for the active cursor position
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Rgb(180, 0, 255))
                    .add_modifier(Modifier::BOLD)
            } else if is_watched {
                // Dimmed to show the episode is already watched
                Style::default()
                    .fg(Color::Rgb(90, 90, 110))
                    .bg(Color::Rgb(18, 18, 28))
            } else {
                Style::default()
                    .fg(Color::Rgb(180, 180, 180))
                    .bg(Color::Rgb(25, 25, 35))
            };

            let pill = Paragraph::new(Span::styled(label, style));
            frame.render_widget(
                pill,
                Rect { x, y, width: pill_width, height: 1 },
            );
            x += pill_width;
        }
        y += 1;
    }

    // Scrollbar if episodes exceed visible area
    if rows_needed > visible_rows {
        let mut scrollbar_state = ScrollbarState::new(rows_needed)
            .position(offset_rows);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            inner,
            &mut scrollbar_state,
        );
    }
}
