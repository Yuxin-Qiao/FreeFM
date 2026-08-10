# Homebrew Formula for FreeFM
# To publish: copy to Yuxin-Qiao/homebrew-tap/Formula/freefm.rb and update checksums
class Freefm < Formula
  desc "Native Rust CLI for safely syncing free-playable NetEase Private FM tracks"
  homepage "https://github.com/Yuxin-Qiao/FreeFM"
  version "0.1.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/Yuxin-Qiao/FreeFM/releases/download/v0.1.0/freefm-v0.1.0-darwin-arm64.tar.gz"
      # sha256 "<checksum-darwin-arm64>"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/Yuxin-Qiao/FreeFM/releases/download/v0.1.0/freefm-v0.1.0-linux-x86_64.tar.gz"
      # sha256 "<checksum-linux-x86_64>"
    elsif Hardware::CPU.arm?
      url "https://github.com/Yuxin-Qiao/FreeFM/releases/download/v0.1.0/freefm-v0.1.0-linux-arm64.tar.gz"
      # sha256 "<checksum-linux-arm64>"
    end
  end

  def install
    bin.install "freefm"
  end

  test do
    assert_match "FreeFM", shell_output("#{bin}/freefm --version")
  end
end
