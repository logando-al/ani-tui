# ani-tui Release Checklist

Use this checklist before publishing a production release tag.

## Scope

- Stable release target: `v1.x.y`
- Official release source: GitHub Releases
- Primary macOS install path: Homebrew tap
- Primary Linux install path: GitHub binary or `cargo install`

## Pre-Release Validation

```bash
cargo check
cargo test
```

Manual smoke test:

- Launch in Ghostty
- Verify `Home`, `Detail`, `Search`, `Help`, `Settings`, and `Setup` overlays
- Verify banner and detail cover rendering
- Verify `Home` row cards remain stable with halfblock covers
- Verify `Because You Watched` and `More Like This` populate from local heuristic scoring
- Verify detached playback opens the external player
- Verify setup overlay appears when dependencies are missing

## Versioning

1. Update the version in `Cargo.toml`
2. Confirm the README install instructions match the current release plan
3. Confirm the Homebrew formula template version and checksum placeholders are ready to update

## Tag + Release

1. Commit all release-ready changes
2. Create the tag:

```bash
git tag v1.0.1
git push origin v1.0.1
```

3. Wait for the GitHub Actions release workflow to build and attach:
   - `ani-tui-macos-aarch64`
   - `ani-tui-macos-x86_64`
   - `ani-tui-linux-x86_64`

## Release Notes Checklist

Include:

- Stable TUI browsing, search, and resume flow
- Heuristic recommendation engine:
  - `Because You Watched`
  - `More Like This`
  - local-first scoring from watch history, genre overlap, format, recency, and cached popularity rows
- Settings overlay (`s`)
- Setup / dependency checks (`!`)
- Supported players: `mpv`, `iina`, `vlc`
- Runtime dependency note: `ani-cli` is still required

Do not position the release as a full self-contained streamer.

## Post-Release

1. Update the Homebrew tap formula with the new version and SHA256
2. Verify install from:
   - Homebrew on macOS
   - release binary on Linux
3. Open a fresh shell and verify:

```bash
ani-tui
```

4. Confirm the setup overlay and help overlay reflect the released keybindings
