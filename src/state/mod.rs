//! Application state — the single source of truth for the TUI.

use crate::db::cache::Anime;
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};
use std::collections::HashSet;

/// Which screen is currently active.
#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    /// Home screen: category rows + featured banner
    Home,
    /// Detail screen: full info + episode list for a selected anime
    Detail,
    /// Playback query picker overlay shown on top of Detail
    PlaybackQuery,
    /// Playback screen: log stream + controls
    Playback,
    /// Search overlay (shown on top of Home or Detail)
    Search,
    /// Help overlay
    Help,
}

/// Which category row the cursor is on (Home screen).
#[derive(Debug, Clone, PartialEq)]
pub enum CategoryRow {
    ContinueWatching,
    Watchlist,
    Trending,
    Popular,
    TopRated,
    Seasonal,
}

/// The full application state passed to every render call.
pub struct AppState {
    /// Currently active screen
    pub screen:           Screen,

    /// Home screen: which row the cursor is on
    pub active_row:       CategoryRow,

    /// Home screen: horizontal card index per row
    pub row_offsets:      std::collections::HashMap<String, usize>,

    /// Detail screen: the anime being viewed
    pub selected_anime:   Option<Anime>,

    /// Detail screen: episode list (1..=N generated from anime.episodes)
    pub episode_list:     Vec<u32>,

    /// Detail screen: which episode is highlighted
    pub selected_episode: Option<u32>,

    /// Detail screen: episode list scroll offset
    pub episode_offset:   usize,

    /// Search overlay: current input text
    pub search_query:     String,

    /// Search overlay: results list
    pub search_results:   Vec<Anime>,

    /// Search overlay: which result is highlighted
    pub search_cursor:    usize,

    /// Playback query picker: candidate search queries to send to ani-cli
    pub playback_queries: Vec<String>,

    /// Playback query picker: selected query index
    pub playback_query_cursor: usize,

    /// Playback: oneshot sender to stop the background player task
    pub player_stop:      Option<tokio::sync::oneshot::Sender<()>>,

    /// Playback: title + episode currently playing (for display)
    pub now_playing:      Option<String>,

    /// Playback: log lines from ani-cli stdout/stderr
    pub playback_logs:    Vec<String>,

    /// Detail: whether the selected anime is in the watchlist
    pub in_watchlist:     bool,

    /// Cover image: ratatui-image stateful protocol (Kitty / Sixel / Halfblock)
    pub cover_state:      Option<Box<dyn StatefulProtocol>>,

    /// Cover image: which anime ID the current cover_state belongs to
    pub cover_anime_id:   Option<i64>,

    /// Terminal image picker (initialized once at startup)
    pub picker:           Option<Picker>,

    /// Toast notification: (message, expiry unix timestamp)
    pub toast:            Option<(String, i64)>,

    /// Whether the app is loading home data (shows spinner)
    pub is_loading:       bool,

    /// Whether the app should quit on next tick
    pub should_quit:      bool,

    /// Detail screen: set of watched episode numbers for the current anime
    pub watched_episodes: HashSet<u32>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            screen:           Screen::Home,
            active_row:       CategoryRow::Trending,
            row_offsets:      std::collections::HashMap::new(),
            selected_anime:   None,
            episode_list:     Vec::new(),
            selected_episode: None,
            episode_offset:   0,
            search_query:     String::new(),
            search_results:   Vec::new(),
            search_cursor:    0,
            playback_queries: Vec::new(),
            playback_query_cursor: 0,
            player_stop:      None,
            now_playing:      None,
            playback_logs:    Vec::new(),
            in_watchlist:     false,
            cover_state:      None,
            cover_anime_id:   None,
            picker:           None,
            toast:            None,
            is_loading:       true,
            should_quit:      false,
            watched_episodes: HashSet::new(),
        }
    }

    /// Show a toast notification (auto-dismissed after 4 seconds).
    pub fn show_toast(&mut self, msg: impl Into<String>, now: i64) {
        self.toast = Some((msg.into(), now + 4));
    }

    /// Check and clear expired toasts. Returns Some(msg) if a toast is active.
    pub fn active_toast(&mut self, now: i64) -> Option<&str> {
        // Check expiry first, then clear (two separate borrows to satisfy NLL)
        let expired = matches!(&self.toast, Some((_, expiry)) if now >= *expiry);
        if expired {
            self.toast = None;
        }
        match &self.toast {
            Some((msg, _)) => Some(msg.as_str()),
            None           => None,
        }
    }

    /// True if this terminal supports real images (Kitty / Sixel / Iterm2).
    pub fn has_image_support(&self) -> bool {
        use ratatui_image::picker::ProtocolType;
        self.picker
            .as_ref()
            .map(|p| p.protocol_type != ProtocolType::Halfblocks)
            .unwrap_or(false)
    }

    /// Stop any currently running player.
    pub fn stop_player(&mut self) {
        if let Some(stop_tx) = self.player_stop.take() {
            let _ = stop_tx.send(());
        }
        self.now_playing = None;
    }

    /// Navigate back: Playback → Detail → Home
    pub fn go_back(&mut self) {
        self.screen = match self.screen {
            Screen::Playback => Screen::Detail,
            Screen::PlaybackQuery => Screen::Detail,
            Screen::Detail   => Screen::Home,
            Screen::Search   => Screen::Home,
            Screen::Help     => Screen::Home,
            Screen::Home     => {
                self.should_quit = true;
                Screen::Home
            }
        };
    }

    /// Open the detail screen for a given anime.
    pub fn open_detail(&mut self, anime: Anime) {
        let total = anime.episodes.unwrap_or(0) as u32;
        let reuse_cover = self.cover_anime_id == Some(anime.id) && self.cover_state.is_some();
        self.episode_list     = (1..=total.max(1)).collect();
        self.selected_episode = Some(1);
        self.episode_offset   = 0;
        self.cover_anime_id   = Some(anime.id);
        if !reuse_cover {
            self.cover_state  = None;            // reset so stale image isn't shown
        }
        self.watched_episodes = HashSet::new();  // will be populated by the caller
        self.selected_anime   = Some(anime);
        self.screen           = Screen::Detail;
    }

    /// Open the search overlay.
    pub fn open_search(&mut self) {
        self.search_query   = String::new();
        self.search_results = Vec::new();
        self.search_cursor  = 0;
        self.screen         = Screen::Search;
    }

    /// Open the playback query picker for the current anime.
    pub fn open_playback_query_picker(&mut self, anime: &Anime) {
        self.playback_queries = anime.playback_queries();
        self.playback_query_cursor = 0;
        self.screen = Screen::PlaybackQuery;
    }

    /// Push a log line to the playback log buffer (capped at 200 lines).
    pub fn push_log(&mut self, line: String) {
        if self.playback_logs.len() >= 200 {
            self.playback_logs.remove(0);
        }
        self.playback_logs.push(line);
    }

    /// Get or default the card offset for a named row.
    pub fn row_offset(&self, row: &str) -> usize {
        *self.row_offsets.get(row).unwrap_or(&0)
    }

    /// Scroll a row right (card index + 1), clamped to max_cards - 1.
    pub fn scroll_row_right(&mut self, row: &str, max_cards: usize) {
        let entry = self.row_offsets.entry(row.to_string()).or_insert(0);
        if *entry + 1 < max_cards {
            *entry += 1;
        }
    }

    /// Scroll a row left (card index - 1), clamped at 0.
    pub fn scroll_row_left(&mut self, row: &str) {
        let entry = self.row_offsets.entry(row.to_string()).or_insert(0);
        *entry = entry.saturating_sub(1);
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_anime(id: i64) -> Anime {
        Anime {
            id,
            title_english: Some("Test".into()),
            title_romaji:  "Test".into(),
            title_native:  None,
            description:   None,
            episodes:      Some(12),
            status:        None,
            season:        None,
            season_year:   None,
            score:         Some(80),
            format:        None,
            genres:        "[]".into(),
            cover_url:     None,
            cover_blob:    None,
            has_dub:       0,
            updated_at:    0,
        }
    }

    #[test]
    fn test_initial_screen_is_home() {
        let state = AppState::new();
        assert_eq!(state.screen, Screen::Home);
    }

    #[test]
    fn test_go_back_from_detail_goes_home() {
        let mut state  = AppState::new();
        state.screen   = Screen::Detail;
        state.go_back();
        assert_eq!(state.screen, Screen::Home);
    }

    #[test]
    fn test_go_back_from_playback_goes_detail() {
        let mut state  = AppState::new();
        state.screen   = Screen::Playback;
        state.go_back();
        assert_eq!(state.screen, Screen::Detail);
    }

    #[test]
    fn test_go_back_from_home_sets_quit() {
        let mut state = AppState::new();
        state.go_back();
        assert!(state.should_quit);
    }

    #[test]
    fn test_go_back_from_search_goes_home() {
        let mut state = AppState::new();
        state.screen  = Screen::Search;
        state.go_back();
        assert_eq!(state.screen, Screen::Home);
    }

    #[test]
    fn test_open_detail_sets_screen_and_anime() {
        let mut state = AppState::new();
        state.open_detail(dummy_anime(42));
        assert_eq!(state.screen, Screen::Detail);
        assert_eq!(state.selected_anime.as_ref().unwrap().id, 42);
        assert_eq!(state.selected_episode, Some(1));
    }

    #[test]
    fn test_open_search_clears_previous_state() {
        let mut state         = AppState::new();
        state.search_query    = "naruto".to_string();
        state.search_cursor   = 5;
        state.open_search();
        assert_eq!(state.search_query,  "");
        assert_eq!(state.search_cursor, 0);
        assert_eq!(state.screen,        Screen::Search);
    }

    #[test]
    fn test_push_log_caps_at_200() {
        let mut state = AppState::new();
        for i in 0..250 {
            state.push_log(format!("line {}", i));
        }
        assert_eq!(state.playback_logs.len(), 200);
        // Oldest lines removed, newest at end
        assert_eq!(state.playback_logs.last().unwrap(), "line 249");
    }

    #[test]
    fn test_row_offset_defaults_to_zero() {
        let state = AppState::new();
        assert_eq!(state.row_offset("trending"), 0);
    }

    #[test]
    fn test_scroll_row_right() {
        let mut state = AppState::new();
        state.scroll_row_right("trending", 10);
        assert_eq!(state.row_offset("trending"), 1);
    }

    #[test]
    fn test_scroll_row_right_clamps_at_max() {
        let mut state = AppState::new();
        state.scroll_row_right("trending", 2);
        state.scroll_row_right("trending", 2);
        state.scroll_row_right("trending", 2); // should not exceed 1
        assert_eq!(state.row_offset("trending"), 1);
    }

    #[test]
    fn test_scroll_row_left_does_not_underflow() {
        let mut state = AppState::new();
        state.scroll_row_left("trending"); // already at 0
        assert_eq!(state.row_offset("trending"), 0);
    }
}
