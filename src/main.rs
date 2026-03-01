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

// ─── Message channel ──────────────────────────────────────────────────────────

/// Messages sent from background tokio tasks to the UI event loop.
enum AppMessage {
    /// Home screen data loaded/refreshed
    HomeData(ui::home::HomeData),
    /// A log line from ani-cli stdout or stderr
    PlaybackLog(String),
    /// ani-cli process exited
    PlaybackDone,
    /// AniList search results
    SearchResults(Vec<db::cache::Anime>),
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

    // ── Startup: kick off background sync ─────────────────────────────────────
    {
        let pool2   = pool.clone();
        let cfg2    = cfg.clone();
        let tx2     = tx.clone();
        tokio::spawn(async move {
            let client = api::anilist::AniListClient::new();
            let now    = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            let result = services::sync::sync_all(
                &pool2,
                &client,
                cfg2.cache.trending_ttl,
                cfg2.cache.stable_ttl,
                now,
            )
            .await;

            match result {
                Ok(data) => { let _ = tx2.send(AppMessage::HomeData(data)).await; }
                Err(e)   => {
                    // On error, send empty home data so UI unblocks
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
                    home_data      = data;
                    state.is_loading = false;
                }
                AppMessage::PlaybackLog(line) => {
                    state.push_log(line);
                }
                AppMessage::PlaybackDone => {
                    state.now_playing = None;
                    state.player_stop = None;
                    // Auto-return to detail screen
                    if state.screen == Screen::Playback {
                        state.screen = Screen::Detail;
                    }
                }
                AppMessage::SearchResults(results) => {
                    state.search_results = results;
                    state.search_cursor  = 0;
                }
            }
        }

        // Draw current screen
        terminal.draw(|frame| {
            match state.screen {
                Screen::Home => {
                    if state.is_loading {
                        render_loading(frame);
                    } else {
                        ui::home::render(frame, &state, &home_data);
                    }
                }
                Screen::Detail   => ui::detail::render(frame, &state),
                Screen::Playback => ui::playback::render(frame, &state),
                Screen::Search   => {
                    // Render home beneath the overlay
                    ui::home::render(frame, &state, &home_data);
                    ui::search::render_overlay(frame, &state);
                }
                Screen::Help => {
                    ui::home::render(frame, &state, &home_data);
                }
            }
        })?;

        // Input: poll with short timeout so background messages are processed promptly
        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                handle_key(
                    key,
                    &mut state,
                    &mut home_data,
                    &pool,
                    &cfg,
                    &tx,
                )
                .await;
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
    // Global keys work on every screen
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.should_quit = true;
            return;
        }
        _ => {}
    }

    match state.screen {
        Screen::Home     => handle_home(key, state, home_data, tx).await,
        Screen::Detail   => handle_detail(key, state, cfg, tx).await,
        Screen::Playback => handle_playback(key, state),
        Screen::Search   => handle_search(key, state, pool, tx).await,
        Screen::Help     => { state.go_back(); }
    }
}

// ── Home screen input ─────────────────────────────────────────────────────────

async fn handle_home(
    key:       event::KeyEvent,
    state:     &mut AppState,
    home_data: &mut ui::home::HomeData,
    tx:        &tokio::sync::mpsc::Sender<AppMessage>,
) {
    use state::CategoryRow::*;

    match key.code {
        KeyCode::Char('q') => state.should_quit = true,
        KeyCode::Esc       => state.should_quit = true,

        // Navigate rows
        KeyCode::Char('j') | KeyCode::Down => {
            state.active_row = match state.active_row {
                ContinueWatching => Trending,
                Trending         => Popular,
                Popular          => TopRated,
                TopRated         => Seasonal,
                Seasonal         => Seasonal,
            };
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.active_row = match state.active_row {
                ContinueWatching => ContinueWatching,
                Trending         => ContinueWatching,
                Popular          => Trending,
                TopRated         => Popular,
                Seasonal         => TopRated,
            };
        }

        // Navigate cards within row
        KeyCode::Char('l') | KeyCode::Right => {
            let (key, max) = active_row_key_max(state, home_data);
            state.scroll_row_right(&key, max);
        }
        KeyCode::Char('h') | KeyCode::Left => {
            let (key, _) = active_row_key_max(state, home_data);
            state.scroll_row_left(&key);
        }

        // Select highlighted card → detail screen
        KeyCode::Enter => {
            if let Some(anime) = active_anime(state, home_data) {
                state.open_detail(anime);
            }
        }

        // Search overlay
        KeyCode::Char('/') => state.open_search(),

        // Help
        KeyCode::Char('?') => state.screen = Screen::Help,

        _ => {}
    }
}

/// Returns the row key string and item count for the currently active row.
fn active_row_key_max(state: &AppState, data: &ui::home::HomeData) -> (String, usize) {
    use state::CategoryRow::*;
    match state.active_row {
        ContinueWatching => ("continue_watching".to_string(), data.continue_watching.len()),
        Trending         => ("trending".to_string(),          data.trending.len()),
        Popular          => ("popular".to_string(),           data.popular.len()),
        TopRated         => ("top_rated".to_string(),         data.top_rated.len()),
        Seasonal         => ("seasonal".to_string(),          data.seasonal.len()),
    }
}

/// Returns the currently highlighted anime card.
fn active_anime(state: &AppState, data: &ui::home::HomeData) -> Option<db::cache::Anime> {
    use state::CategoryRow::*;
    let (key, _) = active_row_key_max(state, data);
    let offset   = state.row_offset(&key);
    let list     = match state.active_row {
        ContinueWatching => &data.continue_watching,
        Trending         => &data.trending,
        Popular          => &data.popular,
        TopRated         => &data.top_rated,
        Seasonal         => &data.seasonal,
    };
    list.get(offset).cloned()
}

// ── Detail screen input ───────────────────────────────────────────────────────

async fn handle_detail(
    key:   event::KeyEvent,
    state: &mut AppState,
    cfg:   &config::Config,
    tx:    &tokio::sync::mpsc::Sender<AppMessage>,
) {
    match key.code {
        KeyCode::Esc => state.go_back(),
        KeyCode::Char('q') => state.go_back(),
        KeyCode::Char('/') => state.open_search(),

        // Navigate episodes
        KeyCode::Char('l') | KeyCode::Right => {
            if let Some(ep) = state.selected_episode {
                let max = state.episode_list.last().copied().unwrap_or(1);
                if ep < max {
                    state.selected_episode = Some(ep + 1);
                    // Scroll episode offset to keep selection visible
                    let pills_per_row = 10usize; // approximate
                    if ep as usize >= state.episode_offset + pills_per_row {
                        state.episode_offset += pills_per_row;
                    }
                }
            }
        }
        KeyCode::Char('h') | KeyCode::Left => {
            if let Some(ep) = state.selected_episode {
                if ep > 1 {
                    state.selected_episode = Some(ep - 1);
                    let pills_per_row = 10usize;
                    if (ep as usize).saturating_sub(1) < state.episode_offset {
                        state.episode_offset = state.episode_offset.saturating_sub(pills_per_row);
                    }
                }
            }
        }

        // Play selected episode
        KeyCode::Enter => {
            if let Some(anime) = &state.selected_anime {
                let ep    = state.selected_episode.unwrap_or(1);
                let title = anime.display_title().to_string();
                let opts  = api::player::PlayOptions {
                    title:   title.clone(),
                    episode: ep,
                    quality: cfg.quality.as_str().to_string(),
                    dub:     cfg.audio_mode == config::AudioMode::Dub,
                };
                start_playback(state, opts, title, ep, tx).await;
            }
        }

        // Add to watchlist
        KeyCode::Char('+') => {
            // Handled externally — watchlist logic lives in main loop (needs pool)
        }

        _ => {}
    }
}

// ── Search overlay input ──────────────────────────────────────────────────────

async fn handle_search(
    key:  event::KeyEvent,
    state: &mut AppState,
    pool:  &sqlx::SqlitePool,
    tx:    &tokio::sync::mpsc::Sender<AppMessage>,
) {
    match key.code {
        KeyCode::Esc => state.go_back(),

        KeyCode::Char(c) => {
            state.search_query.push(c);
            // Search SQLite cache immediately
            let query   = state.search_query.clone();
            let pool2   = pool.clone();
            let tx2     = tx.clone();
            tokio::spawn(async move {
                if let Ok(results) = db::cache::search_cache(&pool2, &query).await {
                    let _ = tx2.send(AppMessage::SearchResults(results)).await;
                }
            });
        }

        KeyCode::Backspace => {
            state.search_query.pop();
            if state.search_query.is_empty() {
                state.search_results.clear();
            } else {
                let query = state.search_query.clone();
                let pool2 = pool.clone();
                let tx2   = tx.clone();
                tokio::spawn(async move {
                    if let Ok(results) = db::cache::search_cache(&pool2, &query).await {
                        let _ = tx2.send(AppMessage::SearchResults(results)).await;
                    }
                });
            }
        }

        KeyCode::Down | KeyCode::Char('j') => {
            if state.search_cursor + 1 < state.search_results.len() {
                state.search_cursor += 1;
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.search_cursor = state.search_cursor.saturating_sub(1);
        }

        KeyCode::Enter => {
            if let Some(anime) = state.search_results.get(state.search_cursor).cloned() {
                state.screen = Screen::Home; // close search
                state.open_detail(anime);
            }
        }

        _ => {}
    }
}

// ── Playback screen input ─────────────────────────────────────────────────────

fn handle_playback(key: event::KeyEvent, state: &mut AppState) {
    match key.code {
        // Stop playback and return to detail
        KeyCode::Char('q') | KeyCode::Esc => {
            state.stop_player();
            state.go_back();
        }
        _ => {}
    }
}

// ─── Playback launcher ────────────────────────────────────────────────────────

/// Spawn ani-cli, stream logs to channel, auto-return on exit.
async fn start_playback(
    state:   &mut AppState,
    opts:    api::player::PlayOptions,
    title:   String,
    episode: u32,
    tx:      &tokio::sync::mpsc::Sender<AppMessage>,
) {
    // Stop any existing player first
    state.stop_player();
    state.playback_logs.clear();
    state.now_playing = Some(format!("{} — Episode {}", title, episode));
    state.screen      = Screen::Playback;

    let mut child = match api::player::spawn_async(&opts) {
        Ok(c) => c,
        Err(e) => {
            state.push_log(format!("Error: {}", e));
            return;
        }
    };

    // Take I/O handles before moving child into wait task
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // oneshot channel: UI sends () to signal kill
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    state.player_stop = Some(stop_tx);

    // Stream stdout
    if let Some(out) = stdout {
        let tx2 = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx2.send(AppMessage::PlaybackLog(line)).await;
            }
        });
    }

    // Stream stderr
    if let Some(err) = stderr {
        let tx3 = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx3.send(AppMessage::PlaybackLog(line)).await;
            }
        });
    }

    // Wait for exit OR stop signal — whichever comes first
    let tx4 = tx.clone();
    tokio::spawn(async move {
        tokio::select! {
            _ = child.wait() => {
                let _ = tx4.send(AppMessage::PlaybackDone).await;
            }
            _ = stop_rx => {
                let _ = child.kill().await;
                let _ = tx4.send(AppMessage::PlaybackDone).await;
            }
        }
    });
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
            Constraint::Percentage(45),
            Constraint::Length(3),
            Constraint::Percentage(55),
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
