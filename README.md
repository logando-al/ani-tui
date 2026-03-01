# ani-tui

A Netflix-style terminal UI for anime, powered by [ani-cli](https://github.com/pystardust/ani-cli).

```
╔══════════════════════════════════════════════════════════════════════════╗
║  ani-tui                                                                 ║
║  ▶ Continue Watching  ──────────────────────────────────────────────     ║
║  ┌────────────────────┐  ┌────────────────────┐  ┌────────────────────┐  ║
║  │  ████████████████  │  │  ████████████████  │  │  ████████████████  │  ║
║  │  ████████████████  │  │  ████████████████  │  │  ████████████████  │  ║
║  │  ████████████████  │  │  ████████████████  │  │  ████████████████  │  ║
║  │  Attack on Titan   │  │  Demon Slayer      │  │  Jujutsu Kaisen    │  ║
║  │  ★9.0 · TV         │  │  ★8.9 · TV         │  │  ★8.7 · TV         │  ║
║  └────────────────────┘  └────────────────────┘  └────────────────────┘  ║
║  🔥 Trending  ───────────────────────────────────────────────────────     ║
╚══════════════════════════════════════════════════════════════════════════╝
```

## Features

- **Netflix-style home screen** — featured banner + category rows (Continue Watching, Watchlist, Trending, Popular, Top Rated, Seasonal)
- **Detail screen** — cover art, metadata, scrollable episode pills with watched indicators
- **Real cover images** — Kitty Graphics Protocol on supported terminals (Ghostty); halfblock fallback everywhere else
- **Playback via ani-cli** — streams log output live, supports next-episode (`n`) without leaving the TUI
- **Watch history** — episodes marked watched on play, persist across sessions
- **Watchlist** — add/remove with `+`, updates the home row immediately
- **Search** — instant SQLite local search + AniList network fallback for full catalogue
- **Offline-capable** — SQLite cache with TTL-based staleness; home screen loads from cache with no network needed
- **Help overlay** (`?`) available on Home and Detail screens

## Prerequisites

Before running `ani-tui`, make sure these runtime dependencies are installed:

- [ani-cli](https://github.com/pystardust/ani-cli) must be installed and available on `$PATH`
- A supported video player must be installed
  - `mpv` is the default player
  - on macOS, `ani-tui` will fall back to `iina` automatically if `mpv` is not installed
  - `vlc` can also be used if you change `player` in the config
- A terminal with truecolor support is recommended
  - Ghostty works, but `ani-tui` currently falls back to halfblock cover rendering there for stability
  - Any other truecolor terminal should also work with the halfblock fallback

### Verify Prerequisites

Run these commands before launching the app:

```bash
which ani-cli
which mpv
```

If you plan to use VLC instead of mpv:

```bash
which vlc
```

If any command prints `not found`, install that dependency first and make sure it is on your shell `$PATH`.

## Requirements

- Rust toolchain (`cargo`) for building from source
- The runtime prerequisites above

## Installation

```bash
git clone https://github.com/logando-al/ani-tui.git
cd ani-tui
cargo build --release
# Copy binary to PATH
cp target/release/ani-tui ~/.local/bin/
```

## Usage

```bash
ani-tui
```

If playback does not start, the most common cause is a missing `ani-cli` or missing player binary.

### Keybindings

#### Home
| Key | Action |
|-----|--------|
| `j` / `k` | Move between category rows |
| `h` / `l` | Scroll cards left / right |
| `Enter` | Open detail screen |
| `/` | Search |
| `?` | Help overlay |
| `r` | Refresh home data |
| `q` / `Esc` | Quit |

#### Detail
| Key | Action |
|-----|--------|
| `h` / `l` | Navigate episodes |
| `Enter` | Play selected episode |
| `+` | Toggle watchlist |
| `/` | Search |
| `?` | Help overlay |
| `Esc` / `q` | Back |

#### Playback
| Key | Action |
|-----|--------|
| `n` | Next episode |
| `q` / `Esc` | Stop and return |

#### Search
| Key | Action |
|-----|--------|
| `j` / `k` | Move cursor |
| `Enter` | Open detail |
| `Esc` | Close |

## Configuration

Config file is created automatically at first run:

- **Linux**: `~/.config/ani-tui/config.toml`
- **macOS**: `~/Library/Application Support/ani-tui/config.toml`

```toml
quality    = "best"      # best | 1080p | 720p | 480p | 360p
audio_mode = "sub"       # sub | dub
player     = "mpv"       # preferred player: mpv (falls back to iina on macOS) | vlc

[cache]
trending_ttl = 86400     # seconds (24h)
stable_ttl   = 604800    # seconds (7 days)
```

## Architecture

```
src/
  api/
    anilist.rs    — AniList GraphQL client (trending, popular, top rated, seasonal, search)
    player.rs     — ani-cli subprocess wrapper
  db/
    mod.rs        — SQLite init + migrations
    cache.rs      — Anime metadata model + read/write
    user.rs       — Watch history, continue watching, watchlist
    sync.rs       — TTL-based cache staleness
  services/
    sync.rs       — Orchestrates AniList → SQLite sync
  ui/
    home.rs       — Netflix home screen
    detail.rs     — Anime detail + episode list
    playback.rs   — Log stream + controls
    search.rs     — Search overlay
    help.rs       — Help overlay + toast notifications
    components/
      cover.rs    — Halfblock cover renderer + Kitty image support
  state/mod.rs    — AppState, Screen enum, navigation helpers
  config.rs       — Config loading/saving
  error.rs        — AppError + Result type
  main.rs         — Event loop, input handlers, background task coordination
migrations/
  001_initial.sql — Database schema
```

## Data Sources

- **Metadata**: [AniList GraphQL API](https://anilist.co/graphql) — no API key required
- **Playback**: [ani-cli](https://github.com/pystardust/ani-cli) — streams from supported providers

## License

MIT
