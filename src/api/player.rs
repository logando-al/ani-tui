//! ani-cli subprocess wrapper.
//! Spawns ani-cli in non-interactive mode for async playback with log streaming.

use crate::error::{AppError, Result};
use std::process::Stdio;
use tokio::process::{Child, Command};

/// Options passed to ani-cli when launching playback.
#[derive(Debug, Clone)]
pub struct PlayOptions {
    /// Title to pass to ani-cli as the search query
    pub title:   String,
    /// Episode number to play
    pub episode: u32,
    /// Quality string e.g. "1080p", "720p", "best"
    pub quality: String,
    /// true = dub, false = sub
    pub dub:     bool,
}

/// Build the ani-cli command arguments for a given play request.
/// Pure function — separated from spawn so it can be tested without forking.
pub fn build_args(opts: &PlayOptions) -> Vec<String> {
    let mut args = Vec::new();

    // Episode selection
    args.push("-e".to_string());
    args.push(opts.episode.to_string());

    // Quality
    args.push("-q".to_string());
    args.push(opts.quality.clone());

    // Dub flag
    if opts.dub {
        args.push("--dub".to_string());
    }

    // No-detach: keep mpv attached so we can read its output from the TUI
    args.push("--no-detach".to_string());

    // Title as final positional argument
    args.push(opts.title.clone());

    args
}

/// Spawn ani-cli asynchronously. stdout + stderr are piped for log streaming.
/// Returns a tokio Child — caller is responsible for reading I/O and waiting.
pub fn spawn_async(opts: &PlayOptions) -> Result<Child> {
    let args = build_args(opts);
    Command::new("ani-cli")
        .args(&args)
        .env("ANI_CLI_NON_INTERACTIVE", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Player(format!("Failed to spawn ani-cli: {e}")))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(title: &str, episode: u32, quality: &str, dub: bool) -> PlayOptions {
        PlayOptions {
            title:   title.to_string(),
            episode,
            quality: quality.to_string(),
            dub,
        }
    }

    #[test]
    fn test_build_args_sub_mode() {
        let args = build_args(&opts("Naruto", 1, "1080p", false));
        assert!(args.contains(&"-e".to_string()));
        assert!(args.contains(&"1".to_string()));
        assert!(args.contains(&"-q".to_string()));
        assert!(args.contains(&"1080p".to_string()));
        assert!(!args.contains(&"--dub".to_string()));
        assert!(args.contains(&"--no-detach".to_string()));
        assert!(args.contains(&"Naruto".to_string()));
    }

    #[test]
    fn test_build_args_dub_mode() {
        let args = build_args(&opts("One Piece", 5, "720p", true));
        assert!(args.contains(&"--dub".to_string()));
        assert!(args.contains(&"5".to_string()));
        assert!(args.contains(&"720p".to_string()));
        assert!(args.contains(&"One Piece".to_string()));
    }

    #[test]
    fn test_build_args_title_is_last() {
        let args = build_args(&opts("Frieren", 3, "best", false));
        assert_eq!(args.last().unwrap(), "Frieren");
    }

    #[test]
    fn test_build_args_episode_follows_flag() {
        let args = build_args(&opts("Test", 7, "720p", false));
        let e_pos = args.iter().position(|a| a == "-e").unwrap();
        assert_eq!(args[e_pos + 1], "7");
    }

    #[test]
    fn test_build_args_quality_follows_flag() {
        let args = build_args(&opts("Test", 1, "480p", false));
        let q_pos = args.iter().position(|a| a == "-q").unwrap();
        assert_eq!(args[q_pos + 1], "480p");
    }

    #[test]
    fn test_build_args_no_detach_always_present() {
        let args_sub = build_args(&opts("Test", 1, "720p", false));
        let args_dub = build_args(&opts("Test", 1, "720p", true));
        assert!(args_sub.contains(&"--no-detach".to_string()));
        assert!(args_dub.contains(&"--no-detach".to_string()));
    }
}
