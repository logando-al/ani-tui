//! ani-cli subprocess wrapper.
//! Spawns ani-cli in non-interactive mode for async playback with log streaming.

use crate::error::{AppError, Result};
use std::{
    env,
    path::{Path, PathBuf},
    process::Stdio,
};
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
    /// Preferred player passed through to ani-cli
    pub player:  String,
}

/// Build the ani-cli command arguments for a given play request.
/// Pure function — separated from spawn so it can be tested without forking.
pub fn build_args(opts: &PlayOptions) -> Vec<String> {
    let mut args = Vec::new();

    // Auto-pick the first ani-cli search match to avoid dropping into the
    // interactive series chooser for ambiguous titles.
    args.push("-S".to_string());
    args.push("1".to_string());

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

    // Title as final positional argument
    args.push(opts.title.clone());

    args
}

/// Resolve the actual player command to give ani-cli.
///
/// Preference order:
/// - `mpv` stays the default when installed
/// - on macOS, `mpv` falls back to `iina` / `iina-cli` when `mpv` is unavailable
/// - `vlc` remains an explicit cross-platform option
pub fn resolve_player(preferred: &str) -> String {
    match preferred {
        "mpv" => {
            if command_exists("mpv") {
                "mpv".to_string()
            } else if let Some(iina) = macos_iina_fallback() {
                iina
            } else {
                "mpv".to_string()
            }
        }
        "iina" => macos_iina_fallback().unwrap_or_else(|| "iina".to_string()),
        "vlc" => "vlc".to_string(),
        other => other.to_string(),
    }
}

/// Detect whether the relevant runtime dependencies are available on this machine.
pub fn detect_dependencies() -> (bool, bool, bool, bool) {
    (
        command_exists("ani-cli"),
        command_exists("mpv"),
        macos_iina_fallback().is_some(),
        command_exists("vlc"),
    )
}

fn command_exists(name: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };

    env::split_paths(&path).any(|dir| executable_exists(&dir.join(name)))
}

fn executable_exists(path: &Path) -> bool {
    path.is_file()
}

fn macos_iina_fallback() -> Option<String> {
    if env::consts::OS != "macos" {
        return None;
    }

    let app_path = PathBuf::from("/Applications/IINA.app/Contents/MacOS/iina-cli");
    if executable_exists(&app_path) {
        return Some(app_path.to_string_lossy().into_owned());
    }

    if command_exists("iina") {
        return Some("iina".to_string());
    }

    None
}

/// Spawn ani-cli asynchronously. stdout + stderr are piped for log streaming.
/// Returns a tokio Child — caller is responsible for reading I/O and waiting.
pub fn spawn_async(opts: &PlayOptions) -> Result<Child> {
    let args = build_args(opts);
    let player = resolve_player(&opts.player);
    Command::new("ani-cli")
        .args(&args)
        .env("ANI_CLI_NON_INTERACTIVE", "1")
        .env("ANI_CLI_PLAYER", player)
        .stdin(Stdio::null())
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
            player:  "mpv".to_string(),
        }
    }

    #[test]
    fn test_build_args_sub_mode() {
        let args = build_args(&opts("Naruto", 1, "1080p", false));
        assert!(args.contains(&"-S".to_string()));
        assert!(args.contains(&"1".to_string()));
        assert!(args.contains(&"-e".to_string()));
        assert!(args.contains(&"1".to_string()));
        assert!(args.contains(&"-q".to_string()));
        assert!(args.contains(&"1080p".to_string()));
        assert!(!args.contains(&"--dub".to_string()));
        assert!(!args.contains(&"--no-detach".to_string()));
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
    fn test_build_args_runs_without_no_detach() {
        let args_sub = build_args(&opts("Test", 1, "720p", false));
        let args_dub = build_args(&opts("Test", 1, "720p", true));
        assert!(!args_sub.contains(&"--no-detach".to_string()));
        assert!(!args_dub.contains(&"--no-detach".to_string()));
    }

    #[test]
    fn test_build_args_select_nth_precedes_title() {
        let args = build_args(&opts("Test", 1, "720p", false));
        let s_pos = args.iter().position(|a| a == "-S").unwrap();
        assert_eq!(args[s_pos + 1], "1");
        assert_eq!(args.last().unwrap(), "Test");
    }

    #[test]
    fn test_resolve_player_keeps_vlc() {
        assert_eq!(resolve_player("vlc"), "vlc");
    }
}
