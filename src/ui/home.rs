//! Home screen — Netflix-style category rows.
//! Renders: Featured banner + Continue Watching + Trending + Popular + Top Rated + Seasonal.

use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::collections::HashMap;

use crate::{
    db::cache::Anime,
    state::{AppState, CategoryRow},
    ui::components::cover::HalfblockCover,
};

/// Width of each anime card in the row (chars)
const CARD_WIDTH: u16  = 22;
/// Height of each anime card (rows)
const CARD_HEIGHT: u16 = 10;
/// Gap between cards
const CARD_GAP: u16    = 1;

/// Render the full home screen.
pub fn render(frame: &mut Frame, state: &mut AppState, categories: &HomeData) {
    let area = frame.area();
    let banner_anime = active_banner_anime(state, categories).or(categories.featured.as_ref());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10), // Featured banner
            Constraint::Length(1),  // Spacer
            Constraint::Min(0),     // Rows
        ])
        .split(area);

    render_featured(frame, chunks[0], state, banner_anime, categories);
    render_rows(frame, chunks[2], state, categories);
}

/// Featured banner at the top — highlights the first trending anime.
fn render_featured(frame: &mut Frame, area: Rect, state: &mut AppState, anime: Option<&Anime>, data: &HomeData) {
    let Some(anime) = anime else {
        let block = Block::default()
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Rgb(20, 20, 20)));
        frame.render_widget(block, area);
        return;
    };

    let banner = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(Color::Rgb(180, 0, 255)))
        .style(Style::default().bg(Color::Rgb(12, 12, 18)));
    let inner = banner.inner(area);
    frame.render_widget(banner, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(20),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(inner);

    let title   = anime.display_title();
    let genres  = anime.genre_list().join(" · ");
    let score   = anime
        .score
        .map(|s| format!("★ {:.1}", s as f32 / 10.0))
        .unwrap_or_else(|| "★ N/A".to_string());
    let eps     = anime
        .episodes
        .map(|e| format!("{} eps", e))
        .unwrap_or_else(|| "? eps".to_string());
    let fmt     = anime.format.as_deref().unwrap_or("TV");
    let status  = anime.status.as_deref().unwrap_or("Unknown");
    let year    = anime
        .season_year
        .map(|value| value.to_string())
        .unwrap_or_else(|| "?".to_string());
    let in_watchlist = data.watchlist.iter().any(|item| item.id == anime.id);
    let watchlist_label = if in_watchlist {
        " - Remove "
    } else {
        " + Watchlist "
    };
    let watched = state
        .banner_progress
        .filter(|(anime_id, _)| *anime_id == anime.id)
        .map(|(_, watched)| watched)
        .unwrap_or(0);
    let progress = format!(
        "{} / {} watched",
        watched,
        anime.episodes.map(|value| value.to_string()).unwrap_or_else(|| "?".to_string())
    );
    let desc    = anime
        .description
        .as_deref()
        .unwrap_or("No description available.")
        .chars()
        .take(120)
        .collect::<String>();

    let cover_frame = if cols[0].width > 4 {
        cols[0].inner(Margin { horizontal: 1, vertical: 0 })
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

    if state.has_image_support() && state.cover_state.is_some() && state.cover_anime_id == Some(anime.id) {
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

    let content = vec![
        Line::from(Span::styled(
            title,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("{}  |  {}  |  {}  |  {}  |  {}", score, eps, fmt, year, status),
            Style::default().fg(Color::Rgb(180, 180, 180)),
        )),
        Line::from(vec![
            Span::styled(
                format!(" {} ", progress),
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Rgb(235, 235, 235))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(genres, Style::default().fg(Color::Rgb(180, 0, 255))),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            desc,
            Style::default().fg(Color::Rgb(200, 200, 200)),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                " Enter Play ",
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
            Span::raw("  "),
            Span::styled(
                " d Detail ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Rgb(210, 210, 210)),
            ),
            Span::raw("  "),
            Span::styled(
                " r Resume ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Rgb(180, 0, 255))
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    let block = Paragraph::new(content)
        .style(Style::default().bg(Color::Rgb(12, 12, 18)))
        .wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(block, cols[2]);
}

fn active_banner_anime<'a>(state: &AppState, data: &'a HomeData) -> Option<&'a Anime> {
    let (row_key, items) = match state.active_row {
        CategoryRow::ContinueWatching => ("continue_watching", &data.continue_watching),
        CategoryRow::Watchlist        => ("watchlist", &data.watchlist),
        CategoryRow::Recommended      => ("recommended", &data.recommended),
        CategoryRow::Trending         => ("trending", &data.trending),
        CategoryRow::Popular          => ("popular", &data.popular),
        CategoryRow::TopRated         => ("top_rated", &data.top_rated),
        CategoryRow::Seasonal         => ("seasonal", &data.seasonal),
    };

    items.get(state.row_offset(row_key))
}

/// Render all category rows.
fn render_rows(frame: &mut Frame, area: Rect, state: &AppState, data: &HomeData) {
    let rows: Vec<(&str, &str, &[Anime])> = vec![
        ("▶ Continue Watching", "continue_watching", &data.continue_watching),
        ("♥ My Watchlist",      "watchlist",         &data.watchlist),
        ("✨ Because You Watched", "recommended",    &data.recommended),
        ("🔥 Trending",         "trending",          &data.trending),
        ("⭐ Popular",          "popular",           &data.popular),
        ("🏆 Top Rated",        "top_rated",         &data.top_rated),
        ("📅 Seasonal",         "seasonal",          &data.seasonal),
    ];

    // Filter out empty rows
    let visible: Vec<_> = rows.into_iter().filter(|(_, _, items)| !items.is_empty()).collect();
    let row_count        = visible.len() as u16;

    if row_count == 0 {
        return;
    }

    // Each row = 1 (label) + CARD_HEIGHT (cards) + 1 (gap)
    let row_height  = 1 + CARD_HEIGHT + 1;
    let max_rows_that_fit = (area.height / row_height).max(1);
    let visible_rows      = row_count.min(max_rows_that_fit);
    let constraints: Vec<Constraint> = (0..visible_rows)
        .map(|_| Constraint::Length(row_height))
        .collect();

    let row_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    // Determine which row key is currently active for highlight
    let active_key = match state.active_row {
        CategoryRow::ContinueWatching => "continue_watching",
        CategoryRow::Watchlist        => "watchlist",
        CategoryRow::Recommended      => "recommended",
        CategoryRow::Trending         => "trending",
        CategoryRow::Popular          => "popular",
        CategoryRow::TopRated         => "top_rated",
        CategoryRow::Seasonal         => "seasonal",
    };

    let visible_rows = visible_rows as usize;
    let active_idx   = visible
        .iter()
        .position(|(_, key, _)| *key == active_key)
        .unwrap_or(0);
    let start_idx    = active_idx.saturating_sub(visible_rows.saturating_sub(1));

    for (i, (label, key, items)) in visible.iter().skip(start_idx).take(visible_rows).enumerate() {
        if i < row_areas.len() {
            render_row(
                frame,
                row_areas[i],
                state,
                label,
                key,
                items,
                *key == active_key,
                &data.recommended_reasons,
            );
        }
    }
}

/// Render a single horizontal category row with a label and cards.
fn render_row(
    frame:     &mut Frame,
    area:      Rect,
    state:     &AppState,
    label:     &str,
    key:       &str,
    items:     &[Anime],
    is_active: bool,
    recommended_reasons: &HashMap<i64, String>,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    // Row label — purple when active, dim white otherwise
    let label_style = if is_active {
        Style::default()
            .fg(Color::Rgb(180, 0, 255))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::Rgb(160, 160, 160))
            .add_modifier(Modifier::BOLD)
    };
    let label_widget = Paragraph::new(Line::from(Span::styled(
        format!(" {}", label),
        label_style,
    )));
    frame.render_widget(label_widget, chunks[0]);

    // Cards — the first visible card is the selected one when this row is active
    let card_area   = chunks[1];
    let offset      = state.row_offset(key);
    let visible_n   = (card_area.width / (CARD_WIDTH + CARD_GAP)).max(1) as usize;
    let visible_items: Vec<&Anime> = items.iter().skip(offset).take(visible_n).collect();

    for (i, anime) in visible_items.iter().enumerate() {
        let x    = card_area.x + i as u16 * (CARD_WIDTH + CARD_GAP);
        let rect = Rect {
            x,
            y:      card_area.y,
            width:  CARD_WIDTH,
            height: CARD_HEIGHT,
        };
        if rect.x + rect.width <= card_area.x + card_area.width {
            let reason = if key == "recommended" {
                recommended_reasons.get(&anime.id).map(String::as_str)
            } else {
                None
            };
            render_card(frame, rect, anime, is_active && i == 0, reason);
        }
    }
}

/// Render a single anime card (cover + title + score).
/// `selected` draws a purple border around the card to indicate it's the active selection.
fn render_card(frame: &mut Frame, area: Rect, anime: &Anime, selected: bool, reason: Option<&str>) {
    if area.height < 3 {
        return;
    }

    // When selected, draw a purple border and shrink the content area inward
    let content_area = if selected {
        let border = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(180, 0, 255)));
        let inner = border.inner(area);
        frame.render_widget(border, area);
        inner
    } else {
        area
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // cover art
            Constraint::Length(1), // title
            Constraint::Length(1), // score + format
        ])
        .split(content_area);

    // Cover (halfblock — real image layer added later)
    frame.render_widget(
        HalfblockCover { anime_id: anime.id, title: anime.display_title() },
        chunks[0],
    );

    // Title
    let title = Paragraph::new(Span::styled(
        anime.short_title(),
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
    ))
    .style(Style::default().bg(Color::Rgb(15, 15, 20)));
    frame.render_widget(title, chunks[1]);

    // Score + format
    let meta_text = match reason {
        Some(label) => format!("• {}", truncate_reason(label, 18)),
        None => {
            let score_str = anime
                .score
                .map(|s| format!("★{:.1}", s as f32 / 10.0))
                .unwrap_or_else(|| "★ N/A".to_string());
            let fmt_str   = anime.format.as_deref().unwrap_or("TV");
            format!("{} · {}", score_str, fmt_str)
        }
    };
    let meta_color = if reason.is_some() {
        Color::Rgb(180, 0, 255)
    } else {
        Color::Rgb(160, 160, 160)
    };
    let meta      = Paragraph::new(Span::styled(
        meta_text,
        Style::default().fg(meta_color),
    ))
    .style(Style::default().bg(Color::Rgb(15, 15, 20)));
    frame.render_widget(meta, chunks[2]);
}

fn truncate_reason(reason: &str, max_chars: usize) -> String {
    let chars: Vec<char> = reason.chars().collect();
    if chars.len() <= max_chars {
        reason.to_string()
    } else {
        let truncated: String = chars[..max_chars.saturating_sub(1)].iter().collect();
        format!("{}…", truncated)
    }
}

/// All data needed to render the home screen.
pub struct HomeData {
    pub featured:          Option<Anime>,
    pub continue_watching: Vec<Anime>,
    pub watchlist:         Vec<Anime>,
    pub recommended:      Vec<Anime>,
    pub recommended_reasons: HashMap<i64, String>,
    pub trending:          Vec<Anime>,
    pub popular:           Vec<Anime>,
    pub top_rated:         Vec<Anime>,
    pub seasonal:          Vec<Anime>,
}

impl HomeData {
    pub fn empty() -> Self {
        Self {
            featured:          None,
            continue_watching: Vec::new(),
            watchlist:         Vec::new(),
            recommended:      Vec::new(),
            recommended_reasons: HashMap::new(),
            trending:          Vec::new(),
            popular:           Vec::new(),
            top_rated:         Vec::new(),
            seasonal:          Vec::new(),
        }
    }
}
