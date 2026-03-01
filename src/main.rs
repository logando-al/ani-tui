mod api;
mod config;
mod db;
mod error;
mod services;
mod state;
mod ui;

use anyhow::Context;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use state::{AppState, Screen};
use std::{io, time::Duration};
use tokio::io::{AsyncBufReadExt, BufReader};

const QUALITY_CHOICES: [config::Quality; 5] = [
    config::Quality::Best,
    config::Quality::P1080,
    config::Quality::P720,
    config::Quality::P480,
    config::Quality::P360,
];

// ─── Message channel ──────────────────────────────────────────────────────────

/// Messages sent from background tokio tasks to the UI event loop.
enum AppMessage {
    /// Home screen data loaded / refreshed
    HomeData(ui::home::HomeData),
    /// A log line from ani-cli stdout or stderr
    PlaybackLog(String),
    /// ani-cli process exited
    PlaybackDone,
    /// AniList search results
    SearchResults(Vec<db::cache::Anime>),
    /// Cover image downloaded and decoded (anime_id, image)
    CoverReady(i64, image::DynamicImage),
    /// Cover image could not be downloaded or decoded (anime_id)
    CoverFailed(i64),
    /// Watchlist changed — send fresh list to update home row immediately
    WatchlistUpdated(Vec<db::cache::Anime>),
    /// Home banner watched-episode count for the active anime
    BannerProgress(i64, usize),
}

// ─── Entry point ──────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── Config + DB ───────────────────────────────────────────────────────────
    let cfg     = config::Config::load().context("Failed to load config")?;
    let db_path = config::Config::db_path().context("Failed to resolve DB path")?;
    let pool    = db::init(db_path.to_str().unwrap_or(":memory:"))
        .await
        .context("Failed to initialise database")?;

    // ── Terminal setup ────────────────────────────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend      = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // ── Message channel (background tasks → UI loop) ──────────────────────────
    let (tx, mut rx) = tokio::sync::mpsc::channel::<AppMessage>(64);

    // ── App state ─────────────────────────────────────────────────────────────
    let mut state     = AppState::new();
    let mut home_data = ui::home::HomeData::empty();

    // ── Image picker (after alternate screen, before event loop) ──────────────
    // guess_protocol() probes the terminal — must happen after EnterAlternateScreen.
    if let Ok(mut picker) = ratatui_image::picker::Picker::from_termios() {
        picker.guess_protocol();
        state.picker = Some(picker);
    }

    // ── Startup: kick off background sync ─────────────────────────────────────
    {
        let pool2 = pool.clone();
        let cfg2  = cfg.clone();
        let tx2   = tx.clone();
        tokio::spawn(async move {
            let client = api::anilist::AniListClient::new();
            let now    = unix_now();
            let result = services::sync::sync_all(
                &pool2, &client,
                cfg2.cache.trending_ttl, cfg2.cache.stable_ttl, now,
            )
            .await;
            match result {
                Ok(data) => { let _ = tx2.send(AppMessage::HomeData(data)).await; }
                Err(e)   => {
                    eprintln!("Sync error: {e}");
                    let _ = tx2.send(AppMessage::HomeData(ui::home::HomeData::empty())).await;
                }
            }
        });
    }

    // ── Main event loop ───────────────────────────────────────────────────────
    loop {
        // Drain background messages
        while let Ok(msg) = rx.try_recv() {
            match msg {
                AppMessage::HomeData(data) => {
                    home_data        = data;
                    state.is_loading = false;
                    refresh_home_cover(&mut state, &home_data, &pool, &tx);
                }
                AppMessage::PlaybackLog(line) => {
                    state.push_log(line);
                }
                AppMessage::PlaybackDone => {
                    state.now_playing = None;
                    state.player_stop = None;
                    if state.screen == Screen::Playback {
                        state.screen = Screen::Detail;
                    }
                }
                AppMessage::SearchResults(results) => {
                    state.search_results = results;
                    state.search_cursor  = 0;
                }
                AppMessage::CoverReady(anime_id, img) => {
                    if state.cover_anime_id == Some(anime_id) {
                        let protocol = state.picker.as_mut().map(|p| p.new_resize_protocol(img));
                        state.cover_state = protocol;
                        state.cover_failed_anime_id = None;
                    }
                }
                AppMessage::CoverFailed(anime_id) => {
                    if state.cover_anime_id == Some(anime_id) {
                        state.cover_failed_anime_id = Some(anime_id);
                    }
                    if state.cover_anime_id == Some(anime_id)
                        && matches!(state.screen, Screen::Detail | Screen::Home)
                    {
                        state.show_toast("Cover image unavailable", unix_now());
                    }
                }
                AppMessage::WatchlistUpdated(list) => {
                    home_data.watchlist = list;
                    let max_idx = home_data.watchlist.len().saturating_sub(1);
                    let offset  = state.row_offset("watchlist").min(max_idx);
                    state.row_offsets.insert("watchlist".to_string(), offset);
                    refresh_home_cover(&mut state, &home_data, &pool, &tx);
                }
                AppMessage::BannerProgress(anime_id, watched) => {
                    if state.cover_anime_id == Some(anime_id) {
                        state.banner_progress = Some((anime_id, watched));
                    }
                }
            }
        }

        // Resolve active toast before entering draw (avoids mutable borrow conflict)
        let now       = unix_now();
        let toast_msg = state.active_toast(now).map(|s| s.to_string());

        // Draw current screen
        terminal.draw(|frame| {
            match state.screen {
                Screen::Home => {
                    if state.is_loading {
                        render_loading(frame);
                    } else {
                        ui::home::render(frame, &mut state, &home_data);
                    }
                }
                Screen::Detail   => ui::detail::render(frame, &mut state),
                Screen::PlaybackQuery => {
                    ui::detail::render(frame, &mut state);
                    ui::play_query::render_overlay(frame, &state);
                }
                Screen::PlaybackOptions => {
                    ui::detail::render(frame, &mut state);
                    ui::play_options::render_overlay(frame, &state);
                }
                Screen::Playback => ui::playback::render(frame, &state),
                Screen::Search   => {
                    // Home beneath the overlay
                    ui::home::render(frame, &mut state, &home_data);
                    ui::search::render_overlay(frame, &state);
                }
                Screen::Help => {
                    // Home beneath the overlay
                    ui::home::render(frame, &mut state, &home_data);
                    ui::help::render_overlay(frame);
                }
            }
            // Toast notification renders on top of everything
            if let Some(ref msg) = toast_msg {
                ui::help::render_toast(frame, msg);
            }
        })?;

        // Poll keyboard with short timeout so background messages stay responsive
        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                handle_key(key, &mut state, &mut home_data, &pool, &cfg, &tx).await;
            }
        }

        if state.should_quit {
            break;
        }
    }

    // ── Cleanup ───────────────────────────────────────────────────────────────
    state.stop_player();
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
    )?;
    terminal.show_cursor()?;
    Ok(())
}

// ─── Input handler ────────────────────────────────────────────────────────────

async fn handle_key(
    key:       event::KeyEvent,
    state:     &mut AppState,
    home_data: &mut ui::home::HomeData,
    pool:      &sqlx::SqlitePool,
    cfg:       &config::Config,
    tx:        &tokio::sync::mpsc::Sender<AppMessage>,
) {
    // Ctrl+C works everywhere
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        state.should_quit = true;
        return;
    }

    match state.screen {
        Screen::Home     => handle_home(key, state, home_data, pool, cfg, tx).await,
        Screen::Detail   => handle_detail(key, state, pool, cfg, tx).await,
        Screen::PlaybackQuery => handle_playback_query(key, state, pool, cfg, tx).await,
        Screen::PlaybackOptions => handle_playback_options(key, state, pool, cfg, tx).await,
        Screen::Playback => handle_playback(key, state, pool, cfg, tx).await,
        Screen::Search   => handle_search(key, state, pool, tx).await,
        Screen::Help     => { state.go_back(); }
    }
}

// ── Home screen ───────────────────────────────────────────────────────────────

async fn handle_home(
    key:       event::KeyEvent,
    state:     &mut AppState,
    home_data: &mut ui::home::HomeData,
    pool:      &sqlx::SqlitePool,
    cfg:       &config::Config,
    tx:        &tokio::sync::mpsc::Sender<AppMessage>,
) {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => state.should_quit = true,

        // Row navigation — skip rows with no content so the cursor never
        // lands on an invisible row where Enter would do nothing.
        KeyCode::Char('j') | KeyCode::Down => {
            state.active_row = next_non_empty_row(&state.active_row, home_data, 1);
            refresh_home_cover(state, home_data, pool, tx);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.active_row = next_non_empty_row(&state.active_row, home_data, -1);
            refresh_home_cover(state, home_data, pool, tx);
        }

        // Card navigation within row
        KeyCode::Char('l') | KeyCode::Right => {
            let (key, max) = active_row_key_max(state, home_data);
            state.scroll_row_right(&key, max);
            refresh_home_cover(state, home_data, pool, tx);
        }
        KeyCode::Char('h') | KeyCode::Left => {
            let (key, _) = active_row_key_max(state, home_data);
            state.scroll_row_left(&key);
            refresh_home_cover(state, home_data, pool, tx);
        }

        // Open detail for highlighted card
        KeyCode::Enter | KeyCode::Char('d') => {
            if let Some(anime) = active_anime(state, home_data) {
                open_detail_from_anime(state, anime, pool, tx).await;
            }
        }

        // Quick resume from Home: open detail, jump to the next unwatched episode,
        // then continue through the normal playback flow.
        KeyCode::Char('r') => {
            if let Some(anime) = active_anime(state, home_data) {
                open_detail_from_anime(state, anime, pool, tx).await;
                begin_playback_flow(state, pool, cfg, false).await;
            }
        }

        KeyCode::Char('/') => state.open_search(),
        KeyCode::Char('?') => state.screen = Screen::Help,

        // Toggle watchlist for the highlighted card directly from Home
        KeyCode::Char('+') => {
            if let Some(anime) = active_anime(state, home_data) {
                let now   = unix_now();
                let in_wl = db::user::is_in_watchlist(pool, anime.id).await.unwrap_or(false);

                if in_wl {
                    if db::user::remove_from_watchlist(pool, anime.id).await.is_ok() {
                        state.show_toast("Removed from watchlist", now);
                        let pool2 = pool.clone();
                        let tx2   = tx.clone();
                        tokio::spawn(async move {
                            let wl = services::sync::load_watchlist(&pool2).await.unwrap_or_default();
                            let _ = tx2.send(AppMessage::WatchlistUpdated(wl)).await;
                        });
                    }
                } else if db::user::add_to_watchlist(pool, anime.id, now).await.is_ok() {
                    state.show_toast("Added to watchlist", now);
                    let pool2 = pool.clone();
                    let tx2   = tx.clone();
                    tokio::spawn(async move {
                        let wl = services::sync::load_watchlist(&pool2).await.unwrap_or_default();
                        let _ = tx2.send(AppMessage::WatchlistUpdated(wl)).await;
                    });
                }
            }
        }

        // Refresh home data (re-sync respecting TTLs)
        KeyCode::Char('R') => {
            state.is_loading = true;
            let pool2 = pool.clone();
            let cfg2  = cfg.clone();
            let tx2   = tx.clone();
            tokio::spawn(async move {
                let client = api::anilist::AniListClient::new();
                let now    = unix_now();
                let result = services::sync::sync_all(
                    &pool2, &client,
                    cfg2.cache.trending_ttl, cfg2.cache.stable_ttl, now,
                )
                .await;
                match result {
                    Ok(data) => { let _ = tx2.send(AppMessage::HomeData(data)).await; }
                    Err(e)   => {
                        eprintln!("Refresh error: {e}");
                        let _ = tx2.send(AppMessage::HomeData(ui::home::HomeData::empty())).await;
                    }
                }
            });
        }

        _ => {}
    }
}

/// Row key string + item count for the active row.
fn active_row_key_max(state: &AppState, data: &ui::home::HomeData) -> (String, usize) {
    use state::CategoryRow::*;
    match state.active_row {
        ContinueWatching => ("continue_watching".to_string(), data.continue_watching.len()),
        Watchlist        => ("watchlist".to_string(),         data.watchlist.len()),
        Recommended      => ("recommended".to_string(),       data.recommended.len()),
        Trending         => ("trending".to_string(),          data.trending.len()),
        Popular          => ("popular".to_string(),           data.popular.len()),
        TopRated         => ("top_rated".to_string(),         data.top_rated.len()),
        Seasonal         => ("seasonal".to_string(),          data.seasonal.len()),
    }
}

/// Currently highlighted anime card.
fn active_anime(state: &AppState, data: &ui::home::HomeData) -> Option<db::cache::Anime> {
    use state::CategoryRow::*;
    let (key, _) = active_row_key_max(state, data);
    let offset   = state.row_offset(&key);
    let list     = match state.active_row {
        ContinueWatching => &data.continue_watching,
        Watchlist        => &data.watchlist,
        Recommended      => &data.recommended,
        Trending         => &data.trending,
        Popular          => &data.popular,
        TopRated         => &data.top_rated,
        Seasonal         => &data.seasonal,
    };
    list.get(offset).cloned()
}

async fn open_detail_from_anime(
    state: &mut AppState,
    anime: db::cache::Anime,
    pool:  &sqlx::SqlitePool,
    tx:    &tokio::sync::mpsc::Sender<AppMessage>,
) {
    let related = services::sync::load_more_like_this(pool, &anime).await.unwrap_or_default();
    let in_wl = db::user::is_in_watchlist(pool, anime.id).await.unwrap_or(false);
    let watched = db::user::get_watched_episodes(pool, anime.id).await.unwrap_or_default();
    state.in_watchlist = in_wl;
    if state.cover_anime_id != Some(anime.id) || state.cover_state.is_none() {
        state.cover_failed_anime_id = None;
        trigger_cover_download(anime.clone(), pool.clone(), tx.clone());
    }
    state.open_detail(anime);
    state.set_watched_episodes(watched.into_iter().map(|e| e as u32).collect());
    state.detail_recommendation_reasons = related
        .iter()
        .map(|(anime, reason)| (anime.id, reason.clone()))
        .collect();
    state.detail_recommendations = related
        .into_iter()
        .map(|(anime, _)| anime)
        .collect();
}

async fn begin_playback_flow(
    state: &mut AppState,
    pool:  &sqlx::SqlitePool,
    cfg:   &config::Config,
    force_resume: bool,
) {
    let Some(anime) = state.selected_anime.clone() else {
        return;
    };

    if force_resume {
        state.selected_episode = Some(state.next_unwatched_episode());
    }

    let default_audio = db::user::get_audio_mode(pool, anime.id)
        .await
        .unwrap_or(None)
        .unwrap_or_else(|| cfg.audio_mode.clone());
    let use_dub = anime.has_dub() && default_audio == config::AudioMode::Dub;

    if let Some(saved_query) = db::user::get_playback_query(pool, anime.id).await.unwrap_or(None) {
        state.open_playback_options(
            saved_query,
            quality_index(&cfg.quality),
            use_dub,
        );
    } else if anime.playback_queries().len() > 1 {
        state.open_playback_query_picker(&anime);
    } else {
        state.open_playback_options(
            anime.playback_query(),
            quality_index(&cfg.quality),
            use_dub,
        );
    }
}

/// Ensure the Home banner cover follows the currently highlighted anime.
fn refresh_home_cover(
    state: &mut AppState,
    data: &ui::home::HomeData,
    pool: &sqlx::SqlitePool,
    tx:   &tokio::sync::mpsc::Sender<AppMessage>,
) {
    let Some(anime) = active_anime(state, data).or_else(|| data.featured.clone()) else {
        return;
    };

    if state.cover_anime_id == Some(anime.id) {
        if state.banner_progress.is_none() {
            trigger_banner_progress(anime.id, pool.clone(), tx.clone());
        }
        return;
    }

    let anime_id = anime.id;
    state.cover_anime_id = Some(anime_id);
    state.cover_state    = None;
    state.cover_failed_anime_id = None;
    state.banner_progress = None;
    trigger_cover_download(anime, pool.clone(), tx.clone());
    trigger_banner_progress(anime_id, pool.clone(), tx.clone());
}

fn trigger_banner_progress(
    anime_id: i64,
    pool: sqlx::SqlitePool,
    tx: tokio::sync::mpsc::Sender<AppMessage>,
) {
    tokio::spawn(async move {
        let watched = db::user::get_watched_episodes(&pool, anime_id)
            .await
            .map(|eps| eps.len())
            .unwrap_or(0);
        let _ = tx.send(AppMessage::BannerProgress(anime_id, watched)).await;
    });
}

fn quality_label(idx: usize) -> &'static str {
    match QUALITY_CHOICES.get(idx) {
        Some(quality) => quality.as_str(),
        None => config::Quality::P1080.as_str(),
    }
}

fn quality_index(quality: &config::Quality) -> usize {
    QUALITY_CHOICES
        .iter()
        .position(|candidate| candidate == quality)
        .unwrap_or(1)
}

/// Item count for a given category row.
fn row_len(row: &state::CategoryRow, data: &ui::home::HomeData) -> usize {
    use state::CategoryRow::*;
    match row {
        ContinueWatching => data.continue_watching.len(),
        Watchlist        => data.watchlist.len(),
        Recommended      => data.recommended.len(),
        Trending         => data.trending.len(),
        Popular          => data.popular.len(),
        TopRated         => data.top_rated.len(),
        Seasonal         => data.seasonal.len(),
    }
}

/// Walk the row order in `direction` (+1 = down, -1 = up) and return the first
/// row that has at least one item.  Returns `current` unchanged if no such row
/// exists in that direction (i.e., already at boundary or all rows are empty).
fn next_non_empty_row(
    current:   &state::CategoryRow,
    data:      &ui::home::HomeData,
    direction: i32,
) -> state::CategoryRow {
    use state::CategoryRow::*;
    let order: &[state::CategoryRow] = &[
        ContinueWatching, Watchlist, Recommended, Trending, Popular, TopRated, Seasonal,
    ];
    let pos = order.iter().position(|r| r == current).unwrap_or(2) as i32;
    let mut i = pos + direction;
    while i >= 0 && i < order.len() as i32 {
        let candidate = &order[i as usize];
        if row_len(candidate, data) > 0 {
            return candidate.clone();
        }
        i += direction;
    }
    current.clone()
}

// ── Detail screen ─────────────────────────────────────────────────────────────

async fn handle_detail(
    key:   event::KeyEvent,
    state: &mut AppState,
    pool:  &sqlx::SqlitePool,
    cfg:   &config::Config,
    tx:    &tokio::sync::mpsc::Sender<AppMessage>,
) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => state.go_back(),
        KeyCode::Char('/')                => state.open_search(),
        KeyCode::Char('?')               => state.screen = Screen::Help,
        KeyCode::Tab => {
            if !state.detail_recommendations.is_empty() {
                state.detail_focus = match state.detail_focus {
                    state::DetailFocus::Episodes => state::DetailFocus::Related,
                    state::DetailFocus::Related => state::DetailFocus::Episodes,
                };
            }
        }

        // Episode navigation
        KeyCode::Char('l') | KeyCode::Right => {
            match state.detail_focus {
                state::DetailFocus::Episodes => {
                    if let Some(ep) = state.selected_episode {
                        let max           = state.episode_list.last().copied().unwrap_or(1);
                        let pills_per_row = 10usize;
                        if ep < max {
                            state.selected_episode = Some(ep + 1);
                            if ep as usize >= state.episode_offset + pills_per_row {
                                state.episode_offset += pills_per_row;
                            }
                        }
                    }
                }
                state::DetailFocus::Related => {
                    if state.detail_related_cursor + 1 < state.detail_recommendations.len() {
                        state.detail_related_cursor += 1;
                    }
                }
            }
        }
        KeyCode::Char('h') | KeyCode::Left => {
            match state.detail_focus {
                state::DetailFocus::Episodes => {
                    if let Some(ep) = state.selected_episode {
                        let pills_per_row = 10usize;
                        if ep > 1 {
                            state.selected_episode = Some(ep - 1);
                            if (ep as usize).saturating_sub(1) < state.episode_offset {
                                state.episode_offset = state.episode_offset.saturating_sub(pills_per_row);
                            }
                        }
                    }
                }
                state::DetailFocus::Related => {
                    state.detail_related_cursor = state.detail_related_cursor.saturating_sub(1);
                }
            }
        }

        // Play
        KeyCode::Enter => {
            if state.detail_focus == state::DetailFocus::Related {
                let origin_title = state
                    .selected_anime
                    .as_ref()
                    .map(|anime| anime.display_title().to_string());
                if let Some(anime) = state
                    .detail_recommendations
                    .get(state.detail_related_cursor)
                    .cloned()
                {
                    open_detail_from_anime(state, anime, pool, tx).await;
                    state.detail_origin_title = origin_title;
                }
            } else {
                begin_playback_flow(state, pool, cfg, false).await;
            }
        }

        KeyCode::Char('n') => {
            if state.detail_focus == state::DetailFocus::Episodes {
                let current = state.selected_episode.unwrap_or(1);
                let max = state.episode_list.last().copied().unwrap_or(1);
                if current < max {
                    state.selected_episode = Some(current + 1);
                    begin_playback_flow(state, pool, cfg, false).await;
                }
            }
        }

        // Watchlist toggle — updates home row immediately via WatchlistUpdated
        KeyCode::Char('+') => {
            if let Some(anime) = state.selected_anime.clone() {
                let now = unix_now();
                if state.in_watchlist {
                    if db::user::remove_from_watchlist(pool, anime.id).await.is_ok() {
                        state.in_watchlist = false;
                        state.show_toast("Removed from watchlist", now);
                        let pool2 = pool.clone();
                        let tx2   = tx.clone();
                        tokio::spawn(async move {
                            let wl = services::sync::load_watchlist(&pool2).await.unwrap_or_default();
                            let _ = tx2.send(AppMessage::WatchlistUpdated(wl)).await;
                        });
                    }
                } else if db::user::add_to_watchlist(pool, anime.id, now).await.is_ok() {
                    state.in_watchlist = true;
                    state.show_toast("Added to watchlist", now);
                    let pool2 = pool.clone();
                    let tx2   = tx.clone();
                    tokio::spawn(async move {
                        let wl = services::sync::load_watchlist(&pool2).await.unwrap_or_default();
                        let _ = tx2.send(AppMessage::WatchlistUpdated(wl)).await;
                    });
                }
            }
        }

        _ => {}
    }
}

// ── Playback query overlay ─────────────────────────────────────────────────────

async fn handle_playback_query(
    key:   event::KeyEvent,
    state: &mut AppState,
    pool:  &sqlx::SqlitePool,
    cfg:   &config::Config,
    _tx:   &tokio::sync::mpsc::Sender<AppMessage>,
) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => state.go_back(),
        KeyCode::Down | KeyCode::Char('j') => {
            if state.playback_query_cursor + 1 < state.playback_queries.len() {
                state.playback_query_cursor += 1;
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.playback_query_cursor = state.playback_query_cursor.saturating_sub(1);
        }
        KeyCode::Enter => {
            if let Some(anime) = state.selected_anime.clone() {
                let query = state
                    .playback_queries
                    .get(state.playback_query_cursor)
                    .cloned()
                    .unwrap_or_else(|| anime.playback_query());
                let _ = db::user::set_playback_query(pool, anime.id, &query, unix_now()).await;
                let preferred_audio = db::user::get_audio_mode(pool, anime.id)
                    .await
                    .unwrap_or(None)
                    .unwrap_or_else(|| cfg.audio_mode.clone());
                state.open_playback_options(
                    query,
                    quality_index(&cfg.quality),
                    anime.has_dub() && preferred_audio == config::AudioMode::Dub,
                );
            }
        }
        _ => {}
    }
}

async fn handle_playback_options(
    key:   event::KeyEvent,
    state: &mut AppState,
    pool:  &sqlx::SqlitePool,
    cfg:   &config::Config,
    tx:    &tokio::sync::mpsc::Sender<AppMessage>,
) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => state.go_back(),
        KeyCode::Down | KeyCode::Char('j') => {
            if state.playback_quality_cursor + 1 < QUALITY_CHOICES.len() {
                state.playback_quality_cursor += 1;
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.playback_quality_cursor = state.playback_quality_cursor.saturating_sub(1);
        }
        KeyCode::Left | KeyCode::Char('h') | KeyCode::Right | KeyCode::Char('l') => {
            if state.selected_anime.as_ref().is_some_and(|anime| anime.has_dub()) {
                state.pending_dub = !state.pending_dub;
            }
        }
        KeyCode::Enter => {
            if let Some(anime) = state.selected_anime.clone() {
                let ep    = state.selected_episode.unwrap_or(1);
                let title = anime.display_title().to_string();
                let query = state
                    .pending_playback_query
                    .clone()
                    .unwrap_or_else(|| anime.playback_query());
                let audio = if anime.has_dub() && state.pending_dub {
                    config::AudioMode::Dub
                } else {
                    config::AudioMode::Sub
                };
                let _ = db::user::set_audio_mode(pool, anime.id, audio.clone(), unix_now()).await;

                let opts = api::player::PlayOptions {
                    title:   query,
                    episode: ep,
                    quality: quality_label(state.playback_quality_cursor).to_string(),
                    dub:     audio == config::AudioMode::Dub,
                    player:  cfg.player.as_str().to_string(),
                };
                start_playback(state, opts, title, ep, tx, pool).await;
            }
        }
        _ => {}
    }
}

// ── Search overlay ────────────────────────────────────────────────────────────

async fn handle_search(
    key:   event::KeyEvent,
    state: &mut AppState,
    pool:  &sqlx::SqlitePool,
    tx:    &tokio::sync::mpsc::Sender<AppMessage>,
) {
    match key.code {
        KeyCode::Esc => {
            // Clear stale search state so it doesn't bleed through on next open
            state.search_query.clear();
            state.search_results.clear();
            state.search_cursor = 0;
            state.go_back();
        }

        KeyCode::Backspace => {
            state.search_query.pop();
            if state.search_query.is_empty() {
                state.search_results.clear();
                state.search_cursor = 0;
            } else {
                let query = state.search_query.clone();
                let pool2 = pool.clone();
                let tx2   = tx.clone();
                tokio::spawn(async move {
                    search_and_send(&pool2, &query, &tx2).await;
                });
            }
        }

        // Cursor movement uses arrows so letter keys still work for typing.
        KeyCode::Down => {
            if state.search_cursor + 1 < state.search_results.len() {
                state.search_cursor += 1;
            }
        }
        KeyCode::Up => {
            state.search_cursor = state.search_cursor.saturating_sub(1);
        }

        KeyCode::Enter => {
            if let Some(anime) = state.search_results.get(state.search_cursor).cloned() {
                state.screen = Screen::Home; // close search first
                open_detail_from_anime(state, anime, pool, tx).await;
            }
        }

        // Append character to search query (after all specific char patterns)
        KeyCode::Char(c) => {
            state.search_query.push(c);
            let query = state.search_query.clone();
            let pool2 = pool.clone();
            let tx2   = tx.clone();
            tokio::spawn(async move {
                search_and_send(&pool2, &query, &tx2).await;
            });
        }

        _ => {}
    }
}

// ── Playback screen ───────────────────────────────────────────────────────────

async fn handle_playback(
    key:   event::KeyEvent,
    state: &mut AppState,
    pool:  &sqlx::SqlitePool,
    cfg:   &config::Config,
    _tx:   &tokio::sync::mpsc::Sender<AppMessage>,
) {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            state.stop_player();
            state.go_back();
        }
        // Next episode without leaving playback
        KeyCode::Char('n') => {
            if state.selected_anime.is_some() {
                let ep  = state.selected_episode.unwrap_or(1);
                let max = state.episode_list.last().copied().unwrap_or(1);
                if ep < max {
                    let next = ep + 1;
                    state.selected_episode = Some(next);
                    begin_playback_flow(state, pool, cfg, false).await;
                }
            }
        }
        _ => {}
    }
}

// ─── Playback launcher ────────────────────────────────────────────────────────

/// Spawn ani-cli, stream logs to channel, record watch history, auto-return on exit.
async fn start_playback(
    state:   &mut AppState,
    opts:    api::player::PlayOptions,
    title:   String,
    episode: u32,
    tx:      &tokio::sync::mpsc::Sender<AppMessage>,
    pool:    &sqlx::SqlitePool,
) {
    state.stop_player();
    state.playback_logs.clear();
    state.now_playing = Some(format!("{} — Episode {}", title, episode));
    state.last_played = state.now_playing.clone();
    state.last_played_anime_id = state.selected_anime.as_ref().map(|anime| anime.id);
    state.screen      = Screen::Detail;
    state.pending_playback_query = None;

    let mut child = match api::player::spawn_async(&opts) {
        Ok(c)  => c,
        Err(e) => {
            // Return to Detail so the user isn't stranded on an empty Playback screen
            state.screen = Screen::Detail;
            state.show_toast(format!("Playback failed: {e}"), unix_now());
            return;
        }
    };

    state.show_toast(format!("Launching Episode {} in external player", episode), unix_now());

    // Record watch history as soon as playback starts
    if let Some(ref anime) = state.selected_anime {
        let pool2    = pool.clone();
        let anime_id = anime.id;
        let now      = unix_now();
        state.watched_episodes.insert(episode);
        tokio::spawn(async move {
            let _ = db::user::record_watched(&pool2, anime_id, episode as i64, now).await;
        });
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    state.player_stop = Some(stop_tx);

    if let Some(out) = stdout {
        let tx2 = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx2.send(AppMessage::PlaybackLog(line)).await;
            }
        });
    }

    if let Some(err) = stderr {
        let tx3 = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx3.send(AppMessage::PlaybackLog(line)).await;
            }
        });
    }

    let tx4 = tx.clone();
    tokio::spawn(async move {
        tokio::select! {
            _ = child.wait()  => { let _ = tx4.send(AppMessage::PlaybackDone).await; }
            _ = stop_rx       => {
                let _ = child.kill().await;
                let _ = tx4.send(AppMessage::PlaybackDone).await;
            }
        }
    });
}

// ─── Cover image download ─────────────────────────────────────────────────────

/// Spawn a background task to fetch (or decode a cached) cover and send CoverReady.
fn trigger_cover_download(
    anime: db::cache::Anime,
    pool:  sqlx::SqlitePool,
    tx:    tokio::sync::mpsc::Sender<AppMessage>,
) {
    tokio::spawn(async move {
        match download_cover_image(&anime, &pool).await {
            Some(img) => { let _ = tx.send(AppMessage::CoverReady(anime.id, img)).await; }
            None      => { let _ = tx.send(AppMessage::CoverFailed(anime.id)).await; }
        }
    });
}

/// Decode blob from DB or fetch from URL, cache the blob on success.
async fn download_cover_image(
    anime: &db::cache::Anime,
    pool:  &sqlx::SqlitePool,
) -> Option<image::DynamicImage> {
    if let Some(ref blob) = anime.cover_blob {
        return image::load_from_memory(blob).ok();
    }
    let url   = anime.cover_url.as_ref()?;
    let resp  = reqwest::get(url).await.ok()?;
    let bytes = resp.bytes().await.ok()?;
    let img   = image::load_from_memory(&bytes).ok()?;
    let _     = db::cache::store_cover_blob(pool, anime.id, &bytes).await;
    Some(img)
}

// ─── Loading screen ───────────────────────────────────────────────────────────

fn render_loading(frame: &mut ratatui::Frame) {
    use ratatui::{
        layout::{Alignment, Constraint, Direction, Layout},
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::Paragraph,
    };

    let area = frame.area();
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(area);

    let msg = Paragraph::new(vec![
        Line::from(Span::styled(
            "ani-tui",
            Style::default()
                .fg(Color::Rgb(180, 0, 255))
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Fetching content...",
            Style::default().fg(Color::Rgb(120, 120, 140)),
        )),
    ])
    .alignment(Alignment::Center)
    .style(Style::default().bg(Color::Rgb(8, 8, 14)));

    frame.render_widget(
        Paragraph::new("").style(Style::default().bg(Color::Rgb(8, 8, 14))),
        area,
    );
    frame.render_widget(msg, vert[1]);
}

// ─── Search helper ────────────────────────────────────────────────────────────

/// Search SQLite cache first (fast), then fall back to AniList network for queries
/// with ≥ 3 characters if local results are sparse. Sends SearchResults messages.
async fn search_and_send(
    pool:  &sqlx::SqlitePool,
    query: &str,
    tx:    &tokio::sync::mpsc::Sender<AppMessage>,
) {
    // Local search — always runs first for instant results
    if let Ok(results) = db::cache::search_cache(pool, query).await {
        let _ = tx.send(AppMessage::SearchResults(results)).await;
    }

    // Network fallback: only when query is long enough and worth the round trip
    if query.len() < 3 {
        return;
    }
    let client = api::anilist::AniListClient::new();
    let now    = unix_now();
    if let Ok(net_results) = client.search(query, now).await {
        // Cache the network results so future local searches benefit
        for anime in &net_results {
            let _ = db::cache::upsert_anime(pool, anime).await;
        }
        // Re-run local search to merge with freshly cached network results
        if let Ok(merged) = db::cache::search_cache(pool, query).await {
            let _ = tx.send(AppMessage::SearchResults(merged)).await;
        }
    }
}

// ─── Utilities ────────────────────────────────────────────────────────────────

/// Current Unix timestamp in seconds.
fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
