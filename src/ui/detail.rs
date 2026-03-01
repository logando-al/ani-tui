//! Detail screen — full anime info + scrollable episode list.

use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

use crate::{
    db::cache::Anime,
    state::{AppState, DetailFocus},
    ui::components::cover::HalfblockCover,
};

/// Render the detail screen.
pub fn render(frame: &mut Frame, state: &mut AppState) {
    let Some(anime) = state.selected_anime.clone() else {
        return;
    };

    let area = frame.area();
    let action_label = match state.selected_episode.unwrap_or(1) {
        1 => " Start E1 ".to_string(),
        ep => format!(" Continue E{} ", ep),
    };

    // Back hint
    let hint = Paragraph::new(Line::from(vec![
        Span::styled(" ← Esc", Style::default().fg(Color::Rgb(120, 120, 120))),
        Span::raw("  "),
        Span::styled("Enter", Style::default().fg(Color::Rgb(180, 0, 255))),
        Span::raw(" "),
        Span::styled(action_label, Style::default().fg(Color::Rgb(220, 220, 220))),
        Span::raw("  "),
        Span::styled("+", Style::default().fg(Color::Rgb(180, 0, 255))),
        Span::raw(" Watchlist  "),
        Span::styled("h/l", Style::default().fg(Color::Rgb(180, 0, 255))),
        Span::raw(" Episode  "),
        Span::styled("n", Style::default().fg(Color::Rgb(180, 0, 255))),
        Span::raw(" Next  "),
        Span::styled("Tab", Style::default().fg(Color::Rgb(180, 0, 255))),
        Span::raw(" Focus related  "),
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
            Constraint::Length(if state.detail_recommendations.is_empty() { 0 } else { 5 }),
            Constraint::Min(0),     // episode list
        ])
        .split(area);

    frame.render_widget(hint, chunks[0]);
    render_info(frame, chunks[1], state, &anime);
    if !state.detail_recommendations.is_empty() {
        render_related(frame, chunks[2], state);
    }
    let episodes_idx = if state.detail_recommendations.is_empty() { 2 } else { 3 };
    render_episodes(frame, chunks[episodes_idx], state, &anime);
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
    } else if state.has_image_support() && state.cover_anime_id == Some(anime.id) {
        let label = if state.cover_failed_anime_id == Some(anime.id) {
            "Cover unavailable"
        } else {
            "Loading cover..."
        };
        let loading = Paragraph::new(label)
            .style(Style::default().fg(Color::Rgb(160, 160, 180)).bg(Color::Rgb(14, 14, 22)))
            .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(loading, cover_inner);
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
    render_metadata(frame, cols[1], state, anime);
}

fn render_metadata(frame: &mut Frame, area: Rect, state: &AppState, anime: &Anime) {
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
        .take(220)
        .collect::<String>();
    let dub_tag = if anime.has_dub() { "  Sub + Dub" } else { "  Sub only" };
    let play_label = match state.selected_episode.unwrap_or(1) {
        1 => " Start E1 ".to_string(),
        ep => format!(" Continue E{} ", ep),
    };
    let watchlist_label = if state.in_watchlist {
        " - Remove "
    } else {
        " + Watchlist "
    };

    let playback_status = if state.now_playing.is_some()
        && state.last_played_anime_id == Some(anime.id)
    {
        state.now_playing.as_deref()
    } else if state.last_played_anime_id == Some(anime.id) {
        state.last_played.as_deref()
    } else {
        None
    };

    let mut lines = Vec::new();
    if let Some(origin) = state.detail_origin_title.as_deref() {
        lines.push(Line::from(vec![
            Span::styled(
                " From ",
                Style::default().fg(Color::Black).bg(Color::Rgb(180, 0, 255)),
            ),
            Span::raw(" "),
            Span::styled(origin, Style::default().fg(Color::Rgb(180, 180, 200))),
        ]));
        lines.push(Line::from(""));
    }

    lines.extend([
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
    ]);
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            play_label,
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            watchlist_label,
            Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(60, 60, 60)),
        ),
    ]));

    if let Some(status_line) = playback_status {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                " Playback ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Rgb(180, 0, 255))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(status_line, Style::default().fg(Color::Rgb(210, 210, 210))),
        ]));
    }

    let para = Paragraph::new(lines)
        .block(Block::default().style(Style::default().bg(Color::Rgb(10, 10, 16))))
        .wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(para, area);
}

fn render_related(frame: &mut Frame, area: Rect, state: &AppState) {
    let block = Block::default()
        .title(Span::styled(
            " More Like This ",
            Style::default().fg(Color::Rgb(220, 220, 220)).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::Rgb(60, 60, 80)))
        .style(Style::default().bg(Color::Rgb(10, 10, 16)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 || inner.width < 12 {
        return;
    }

    let card_width: u16 = 24;
    let gap: u16 = 1;
    let visible = (inner.width / (card_width + gap)).max(1) as usize;

    for (idx, anime) in state.detail_recommendations.iter().take(visible).enumerate() {
        let x = inner.x + idx as u16 * (card_width + gap);
        if x + card_width > inner.x + inner.width {
            break;
        }

        let rect = Rect {
            x,
            y: inner.y,
            width: card_width,
            height: inner.height.min(3),
        };

        let reason = state
            .detail_recommendation_reasons
            .get(&anime.id)
            .map(String::as_str)
            .unwrap_or("Picked for you");

        let lines = vec![
            Line::from(Span::styled(
                anime.short_title(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                reason,
                Style::default().fg(Color::Rgb(180, 0, 255)),
            )),
        ];

        let is_selected = state.detail_focus == DetailFocus::Related && idx == state.detail_related_cursor;
        let item = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(if is_selected {
                        Style::default().fg(Color::Rgb(180, 0, 255))
                    } else {
                        Style::default().fg(Color::Rgb(45, 45, 65))
                    })
                    .style(Style::default().bg(Color::Rgb(14, 14, 20))),
            )
            .wrap(ratatui::widgets::Wrap { trim: true });
        frame.render_widget(item, rect);
    }
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
