class AniTui < Formula
  desc "Terminal UI for anime with local heuristic recommendations"
  homepage "https://github.com/logando-al/ani-tui"
  version "1.0.4"

  if Hardware::CPU.arm?
    url "https://github.com/logando-al/ani-tui/releases/download/v1.0.4/ani-tui-macos-aarch64"
    sha256 "1f1ab55408653d82f3d4f6504053fb7585391df2613192518055bfcbf41cf397"
  else
    url "https://github.com/logando-al/ani-tui/releases/download/v1.0.4/ani-tui-macos-x86_64"
    sha256 "e8ab05236332226719b6160dc764bf8bb751fdf518ee64f9c8ef4409d1d1ef5d"
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
