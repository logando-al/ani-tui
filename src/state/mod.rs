//! Application state — the single source of truth for the TUI.

use crate::db::cache::Anime;
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};
use std::collections::{HashMap, HashSet};

/// Which screen is currently active.
#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    /// Home screen: category rows + featured banner
    Home,
    /// Detail screen: full info + episode list for a selected anime
    Detail,
    /// Playback query picker overlay shown on top of Detail
    PlaybackQuery,
    /// Playback options overlay shown on top of Detail
    PlaybackOptions,
    /// Playback screen: log stream + controls
    Playback,
    /// Search overlay (shown on top of Home or Detail)
    Search,
    /// Help overlay
    Help,
    /// Settings overlay
    Settings,
    /// Dependency / onboarding overlay
    Setup,
}

/// Which category row the cursor is on (Home screen).
#[derive(Debug, Clone, PartialEq)]
pub enum CategoryRow {
    ContinueWatching,
    Watchlist,
    Recommended,
    Trending,
    Popular,
    TopRated,
    Seasonal,
}

/// Which section is currently focused on the Detail screen.
#[derive(Debug, Clone, PartialEq)]
pub enum DetailFocus {
    Episodes,
    Related,
}

#[derive(Clone)]
struct DetailSnapshot {
    anime: Anime,
    in_watchlist: bool,
    watched_episodes: HashSet<u32>,
    detail_recommendations: Vec<Anime>,
    detail_recommendation_reasons: HashMap<i64, String>,
    detail_focus: DetailFocus,
    detail_related_cursor: usize,
    detail_related_offset: usize,
    detail_origin_title: Option<String>,
    last_played: Option<String>,
    last_played_anime_id: Option<i64>,
}

/// The full application state passed to every render call.
pub struct AppState {
    /// Currently active screen
    pub screen:           Screen,

    /// Base screen to render under overlays
    pub overlay_base:     Screen,

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

    /// Settings overlay: which preference row is focused
    pub settings_cursor: usize,

    /// Playback query picker: candidate search queries to send to ani-cli
    pub playback_queries: Vec<String>,

    /// Playback query picker: selected query index
    pub playback_query_cursor: usize,

    /// Playback options: currently selected quality index
    pub playback_quality_cursor: usize,

    /// Playback options: pending query chosen before launch
    pub pending_playback_query: Option<String>,

    /// Playback options: pending sub/dub mode for the next launch
    pub pending_dub:       bool,

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

    /// Cover image: last anime ID whose image failed to load
    pub cover_failed_anime_id: Option<i64>,

    /// Terminal image picker (initialized once at startup)
    pub picker:           Option<Picker>,

    /// Home banner: watched episode count for the selected anime
    pub banner_progress:  Option<(i64, usize)>,

    /// Detail screen: "More Like This" recommendations
    pub detail_recommendations: Vec<Anime>,

    /// Detail screen: reason labels keyed by anime ID
    pub detail_recommendation_reasons: HashMap<i64, String>,

    /// Detail screen: which area receives h/l and Enter
    pub detail_focus: DetailFocus,

    /// Detail screen: selected "More Like This" card
    pub detail_related_cursor: usize,

    /// Detail screen: horizontal scroll offset for "More Like This"
    pub detail_related_offset: usize,

    /// Detail screen: breadcrumb title when opened from a related recommendation
    pub detail_origin_title: Option<String>,

    /// Detail navigation history when drilling into related anime
    detail_history: Vec<DetailSnapshot>,

    /// Runtime dependency status
    pub has_ani_cli: bool,
    pub has_mpv: bool,
    pub has_iina: bool,
    pub has_vlc: bool,

    /// Toast notification: (message, expiry unix timestamp)
    pub toast:            Option<(String, i64)>,

    /// Whether the app is loading home data (shows spinner)
    pub is_loading:       bool,

    /// Whether the app should quit on next tick
    pub should_quit:      bool,

    /// Detail screen: set of watched episode numbers for the current anime
    pub watched_episodes: HashSet<u32>,

    /// Last launched playback label for the current session
    pub last_played:      Option<String>,

    /// Which anime the last launched playback belongs to
    pub last_played_anime_id: Option<i64>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            screen:           Screen::Home,
            overlay_base:     Screen::Home,
            active_row:       CategoryRow::Trending,
            row_offsets:      std::collections::HashMap::new(),
            selected_anime:   None,
            episode_list:     Vec::new(),
            selected_episode: None,
            episode_offset:   0,
            search_query:     String::new(),
            search_results:   Vec::new(),
            search_cursor:    0,
            settings_cursor:  0,
            playback_queries: Vec::new(),
            playback_query_cursor: 0,
            playback_quality_cursor: 0,
            pending_playback_query: None,
            pending_dub: false,
            player_stop:      None,
            now_playing:      None,
            playback_logs:    Vec::new(),
            in_watchlist:     false,
            cover_state:      None,
            cover_anime_id:   None,
            cover_failed_anime_id: None,
            picker:           None,
            banner_progress:  None,
            detail_recommendations: Vec::new(),
            detail_recommendation_reasons: HashMap::new(),
            detail_focus:      DetailFocus::Episodes,
            detail_related_cursor: 0,
            detail_related_offset: 0,
            detail_origin_title: None,
            detail_history:     Vec::new(),
            has_ani_cli:      true,
            has_mpv:          true,
            has_iina:         false,
            has_vlc:          true,
            toast:            None,
            is_loading:       true,
            should_quit:      false,
            watched_episodes: HashSet::new(),
            last_played:      None,
            last_played_anime_id: None,
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
            Screen::PlaybackOptions => Screen::Detail,
            Screen::Detail   => {
                if self.restore_previous_detail() {
                    Screen::Detail
                } else {
                    Screen::Home
                }
            }
            Screen::Search | Screen::Help | Screen::Settings | Screen::Setup => {
                self.overlay_base.clone()
            }
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
        self.detail_recommendations = Vec::new();
        self.detail_recommendation_reasons = HashMap::new();
        self.detail_focus = DetailFocus::Episodes;
        self.detail_related_cursor = 0;
        self.detail_related_offset = 0;
        self.detail_origin_title = None;
        self.selected_anime   = Some(anime);
        self.screen           = Screen::Detail;
    }

    /// Open the search overlay.
    pub fn open_search(&mut self) {
        self.search_query   = String::new();
        self.search_results = Vec::new();
        self.search_cursor  = 0;
        self.overlay_base   = self.current_base_screen();
        self.screen         = Screen::Search;
    }

    /// Open the help overlay.
    pub fn open_help(&mut self) {
        self.overlay_base = self.current_base_screen();
        self.screen = Screen::Help;
    }

    /// Open the settings overlay.
    pub fn open_settings(&mut self) {
        self.overlay_base = self.current_base_screen();
        self.settings_cursor = 0;
        self.screen = Screen::Settings;
    }

    /// Open the dependency / setup overlay.
    pub fn open_setup(&mut self) {
        self.overlay_base = self.current_base_screen();
        self.screen = Screen::Setup;
    }

    /// Open the playback query picker for the current anime.
    pub fn open_playback_query_picker(&mut self, anime: &Anime) {
        self.playback_queries = anime.playback_queries();
        self.playback_query_cursor = 0;
        self.pending_playback_query = None;
        self.screen = Screen::PlaybackQuery;
    }

    /// Open the playback options overlay after a query has been chosen.
    pub fn open_playback_options(&mut self, query: String, quality_idx: usize, dub: bool) {
        self.pending_playback_query = Some(query);
        self.playback_quality_cursor = quality_idx;
        self.pending_dub = dub;
        self.screen = Screen::PlaybackOptions;
    }

    /// Update watched progress and set the selected episode to the next unwatched.
    pub fn set_watched_episodes(&mut self, watched: HashSet<u32>) {
        self.watched_episodes = watched;
        let next = self.next_unwatched_episode();
        self.selected_episode = Some(next);
        let pills_per_row = 10usize;
        self.episode_offset = next.saturating_sub(1) as usize / pills_per_row * pills_per_row;
    }

    /// Return the next unwatched episode, defaulting to 1 when all known episodes are watched.
    pub fn next_unwatched_episode(&self) -> u32 {
        self.episode_list
            .iter()
            .copied()
            .find(|ep| !self.watched_episodes.contains(ep))
            .or_else(|| self.episode_list.last().copied())
            .unwrap_or(1)
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

    /// Return the base screen that should remain visible under overlays.
    pub fn current_base_screen(&self) -> Screen {
        match self.screen {
            Screen::Detail | Screen::Playback | Screen::PlaybackQuery | Screen::PlaybackOptions => {
                Screen::Detail
            }
            Screen::Search | Screen::Help | Screen::Settings | Screen::Setup => self.overlay_base.clone(),
            Screen::Home => Screen::Home,
        }
    }

    /// Update the cached dependency status.
    pub fn set_dependencies(&mut self, ani_cli: bool, mpv: bool, iina: bool, vlc: bool) {
        self.has_ani_cli = ani_cli;
        self.has_mpv = mpv;
        self.has_iina = iina;
        self.has_vlc = vlc;
    }

    /// True when playback can run with the currently installed tools.
    pub fn has_any_player(&self) -> bool {
        self.has_mpv || self.has_iina || self.has_vlc
    }

    /// Save the current detail screen so a related drill-down can return to it.
    pub fn push_detail_snapshot(&mut self) {
        let Some(anime) = self.selected_anime.clone() else {
            return;
        };

        self.detail_history.push(DetailSnapshot {
            anime,
            in_watchlist: self.in_watchlist,
            watched_episodes: self.watched_episodes.clone(),
            detail_recommendations: self.detail_recommendations.clone(),
            detail_recommendation_reasons: self.detail_recommendation_reasons.clone(),
            detail_focus: self.detail_focus.clone(),
            detail_related_cursor: self.detail_related_cursor,
            detail_related_offset: self.detail_related_offset,
            detail_origin_title: self.detail_origin_title.clone(),
            last_played: self.last_played.clone(),
            last_played_anime_id: self.last_played_anime_id,
        });
    }

    fn restore_previous_detail(&mut self) -> bool {
        let Some(snapshot) = self.detail_history.pop() else {
            return false;
        };

        let total = snapshot.anime.episodes.unwrap_or(0) as u32;
        self.episode_list = (1..=total.max(1)).collect();
        self.selected_anime = Some(snapshot.anime);
        self.in_watchlist = snapshot.in_watchlist;
        self.detail_recommendations = snapshot.detail_recommendations;
        self.detail_recommendation_reasons = snapshot.detail_recommendation_reasons;
        self.detail_focus = snapshot.detail_focus;
        self.detail_related_cursor = snapshot.detail_related_cursor;
        self.detail_related_offset = snapshot.detail_related_offset;
        self.detail_origin_title = snapshot.detail_origin_title;
        self.last_played = snapshot.last_played;
        self.last_played_anime_id = snapshot.last_played_anime_id;
        self.set_watched_episodes(snapshot.watched_episodes);
        true
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
    fn test_set_watched_episodes_selects_next_unwatched() {
        let mut state = AppState::new();
        state.open_detail(dummy_anime(42));
        state.set_watched_episodes([1, 2, 4].into_iter().collect());
        assert_eq!(state.selected_episode, Some(3));
    }

    #[test]
    fn test_next_unwatched_falls_back_to_last_episode_when_complete() {
        let mut state = AppState::new();
        state.open_detail(dummy_anime(42));
        state.set_watched_episodes((1..=12).collect());
        assert_eq!(state.selected_episode, Some(12));
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
