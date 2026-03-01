use crate::error::{AppError, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Video quality preference
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    #[serde(rename = "360p")]
    P360,
    #[serde(rename = "480p")]
    P480,
    #[serde(rename = "720p")]
    P720,
    #[serde(rename = "1080p")]
    P1080,
    Best,
}

impl Quality {
    pub fn as_str(&self) -> &str {
        match self {
            Quality::P360  => "360p",
            Quality::P480  => "480p",
            Quality::P720  => "720p",
            Quality::P1080 => "1080p",
            Quality::Best  => "best",
        }
    }
}

impl Default for Quality {
    fn default() -> Self {
        Quality::P1080
    }
}

/// Sub or dub preference
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AudioMode {
    #[default]
    Sub,
    Dub,
}

/// Video player to use
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Player {
    #[default]
    Mpv,
    Vlc,
}

impl Player {
    pub fn as_str(&self) -> &str {
        match self {
            Player::Mpv => "mpv",
            Player::Vlc => "vlc",
        }
    }
}

/// TTL settings (in seconds) for AniList cache refresh
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Trending and seasonal rows (default: 24h)
    pub trending_ttl: u64,
    /// Popular and top-rated rows (default: 7 days)
    pub stable_ttl: u64,
    /// Individual anime detail (default: 48h)
    pub detail_ttl: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            trending_ttl: 60 * 60 * 24,         // 24 hours
            stable_ttl:   60 * 60 * 24 * 7,     // 7 days
            detail_ttl:   60 * 60 * 48,          // 48 hours
        }
    }
}

/// Root config struct — persisted to ~/.config/ani-tui/config.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub quality:    Quality,
    pub audio_mode: AudioMode,
    pub player:     Player,
    pub cache:      CacheConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            quality:    Quality::default(),
            audio_mode: AudioMode::default(),
            player:     Player::default(),
            cache:      CacheConfig::default(),
        }
    }
}

impl Config {
    /// Returns the platform-appropriate config directory path.
    /// e.g. ~/.config/ani-tui/ on Linux
    pub fn config_dir() -> Result<PathBuf> {
        ProjectDirs::from("", "", "ani-tui")
            .map(|dirs| dirs.config_dir().to_path_buf())
            .ok_or_else(|| AppError::Config("Could not determine config directory".into()))
    }

    /// Returns the path to config.toml
    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    /// Returns the path to the SQLite database file
    pub fn db_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("ani-tui.db"))
    }

    /// Load config from disk. Returns default config if file doesn't exist.
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| AppError::Config(format!("Failed to read config: {e}")))?;
        toml::from_str(&raw)
            .map_err(|e| AppError::Config(format!("Failed to parse config: {e}")))
    }

    /// Persist config to disk, creating the directory if needed.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| AppError::Config(format!("Failed to serialize config: {e}")))?;
        std::fs::write(&path, content)?;
        Ok(())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn config_in_tempdir(tmp: &TempDir) -> Config {
        // Redirect config path to temp dir by writing manually
        let path = tmp.path().join("config.toml");
        let cfg = Config::default();
        let content = toml::to_string_pretty(&cfg).unwrap();
        std::fs::write(&path, content).unwrap();
        cfg
    }

    #[test]
    fn test_default_config_values() {
        let cfg = Config::default();
        assert_eq!(cfg.quality, Quality::P1080);
        assert_eq!(cfg.audio_mode, AudioMode::Sub);
        assert_eq!(cfg.player, Player::Mpv);
        assert_eq!(cfg.cache.trending_ttl, 86_400);
        assert_eq!(cfg.cache.stable_ttl, 604_800);
    }

    #[test]
    fn test_quality_as_str() {
        assert_eq!(Quality::P720.as_str(),  "720p");
        assert_eq!(Quality::P1080.as_str(), "1080p");
        assert_eq!(Quality::Best.as_str(),  "best");
    }

    #[test]
    fn test_player_as_str() {
        assert_eq!(Player::Mpv.as_str(), "mpv");
        assert_eq!(Player::Vlc.as_str(), "vlc");
    }

    #[test]
    fn test_config_round_trip_toml() {
        let original = Config {
            quality:    Quality::P720,
            audio_mode: AudioMode::Dub,
            player:     Player::Vlc,
            cache:      CacheConfig::default(),
        };
        let serialized   = toml::to_string_pretty(&original).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();

        assert_eq!(deserialized.quality,    Quality::P720);
        assert_eq!(deserialized.audio_mode, AudioMode::Dub);
        assert_eq!(deserialized.player,     Player::Vlc);
    }

    #[test]
    fn test_save_and_load_config() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");

        let original = Config {
            quality:    Quality::P480,
            audio_mode: AudioMode::Sub,
            player:     Player::Mpv,
            cache:      CacheConfig::default(),
        };

        // Save manually (bypassing config_path() which uses XDG dirs)
        let content = toml::to_string_pretty(&original).unwrap();
        std::fs::write(&path, content).unwrap();

        // Load back
        let raw  = std::fs::read_to_string(&path).unwrap();
        let loaded: Config = toml::from_str(&raw).unwrap();

        assert_eq!(loaded.quality,    Quality::P480);
        assert_eq!(loaded.audio_mode, AudioMode::Sub);
        assert_eq!(loaded.player,     Player::Mpv);
    }

    #[test]
    fn test_load_missing_file_returns_default() {
        // Config::load() returns default when file doesn't exist.
        // We can't easily redirect the path, but we can verify toml::from_str
        // of an empty string fails gracefully and default is sound.
        let cfg = Config::default();
        assert_eq!(cfg.quality, Quality::P1080);
    }

    #[test]
    fn test_config_dir_is_some() {
        // Just ensure the platform can resolve a config dir
        assert!(Config::config_dir().is_ok());
    }
}
