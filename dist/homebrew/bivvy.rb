# frozen_string_literal: true

class Bivvy < Formula
  desc "Cross-language development environment setup automation"
  homepage "https://bivvy.dev"
  version "1.9.0"
  license "FSL-1.1-Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/bivvy-dev/bivvy/releases/download/#{version}/bivvy-darwin-arm64.tar.gz"
      sha256 "dbd298fbe05d8ce9af8d18f9d785a0a118ba8bd8a0ebf3efa4e122502c14b2f8"
    end
    on_intel do
      url "https://github.com/bivvy-dev/bivvy/releases/download/#{version}/bivvy-darwin-x64.tar.gz"
      sha256 "e1c52ac36c1908535f341071b48efc50492266e7967804c3ad7bc287c7f54a4d"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/bivvy-dev/bivvy/releases/download/#{version}/bivvy-linux-arm64.tar.gz"
      sha256 "51cf78014a374e70ff40ed6f9514aabfad6461dcae48cadfdfa1a1434279aaa6"
    end
    on_intel do
      url "https://github.com/bivvy-dev/bivvy/releases/download/#{version}/bivvy-linux-x64.tar.gz"
      sha256 "5cc61ce7f8fc804abf7960350961840976d3eadc6d9dc9ef8a983d55a1d73042"
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
