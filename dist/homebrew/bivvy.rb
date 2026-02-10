# frozen_string_literal: true

class Bivvy < Formula
  desc "Cross-language development environment setup automation"
  homepage "https://bivvy.dev"
  version "1.0.0"
  license "FSL-1.1-Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/bivvy-dev/bivvy/releases/download/v#{version}/bivvy-darwin-arm64.tar.gz"
      sha256 "a7f2cef3326653e824afb104a7cd17f48c0a859b2ba1548a34564b71ae3a1d01"
    end
    on_intel do
      url "https://github.com/bivvy-dev/bivvy/releases/download/v#{version}/bivvy-darwin-x64.tar.gz"
      sha256 "9c16c472a37b1dd0551e28aa97c28f5ed4ffa29fbb0f8847cd10fd44f1022c2c"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/bivvy-dev/bivvy/releases/download/v#{version}/bivvy-linux-arm64.tar.gz"
      sha256 "2b8ac0c2883863144758dd46a901e100407e7f77142a5c90390cdd37e47e5ede"
    end
    on_intel do
      url "https://github.com/bivvy-dev/bivvy/releases/download/v#{version}/bivvy-linux-x64.tar.gz"
      sha256 "e117ab7fee72573e7635f9daf192fbff1a64bf1b006e2a7eec538482ca430b5d"
    end
  end

  def install
    bin.install "bivvy"
    generate_completions_from_executable(bin/"bivvy", "completions")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/bivvy --version")
  end
end
