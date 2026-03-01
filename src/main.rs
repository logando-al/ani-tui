mod api;
mod config;
mod db;
mod error;
mod state;
mod ui;

use anyhow::Context;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use state::AppState;
use std::io;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── Config ────────────────────────────────────────────────────────────────
    let cfg     = config::Config::load().context("Failed to load config")?;
    let db_path = config::Config::db_path().context("Failed to resolve DB path")?;

    // ── Database ──────────────────────────────────────────────────────────────
    let pool = db::init(db_path.to_str().unwrap_or(":memory:"))
        .await
        .context("Failed to initialise database")?;

    // ── Terminal setup ────────────────────────────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend      = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // ── App state ─────────────────────────────────────────────────────────────
    let mut state     = AppState::new();
    let mut home_data = ui::home::HomeData::empty();

    // ── Event loop ────────────────────────────────────────────────────────────
    loop {
        terminal.draw(|frame| {
            ui::home::render(frame, &state, &home_data);
        })?;

        if event::poll(std::time::Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => state.should_quit = true,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        state.should_quit = true;
                    }
                    KeyCode::Esc => state.go_back(),
                    _ => {}
                }
            }
        }

        if state.should_quit {
            break;
        }
    }

    // ── Cleanup ───────────────────────────────────────────────────────────────
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
    )?;
    terminal.show_cursor()?;

    // suppress unused warnings for now — cfg and pool used in next phase
    let _ = cfg;
    let _ = pool;

    Ok(())
}
