class AniTui < Formula
  desc "Terminal UI for anime with local heuristic recommendations"
  homepage "https://github.com/logando-al/ani-tui"
  version "1.0.0"

  if Hardware::CPU.arm?
    url "https://github.com/logando-al/ani-tui/releases/download/v1.0.0/ani-tui-macos-aarch64"
    sha256 "REPLACE_WITH_MACOS_AARCH64_SHA256"
  else
    url "https://github.com/logando-al/ani-tui/releases/download/v1.0.0/ani-tui-macos-x86_64"
    sha256 "REPLACE_WITH_MACOS_X86_64_SHA256"
  end

  def install
    bin.install Dir["ani-tui-macos-*"].first => "ani-tui"
  end

  def caveats
    <<~EOS
      ani-tui requires external runtime dependencies for playback:

        brew install curl grep aria2 ffmpeg git fzf yt-dlp
        brew install --cask iina

      You must also install ani-cli separately and ensure it is on your PATH.

      In the app:
      - press s for Settings
      - press ! for the Setup / dependency check screen
    EOS
  end

  test do
    assert_predicate bin/"ani-tui", :exist?
  end
end
