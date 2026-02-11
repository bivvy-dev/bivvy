# frozen_string_literal: true

class Bivvy < Formula
  desc "Cross-language development environment setup automation"
  homepage "https://bivvy.dev"
  version "main"
  license "FSL-1.1-Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/bivvy-dev/bivvy/releases/download/#{version}/bivvy-darwin-arm64.tar.gz"
      sha256 "e5153268254e383bcd1ac8d4f489969175d879f335bfa826c551dfa881287256"
    end
    on_intel do
      url "https://github.com/bivvy-dev/bivvy/releases/download/#{version}/bivvy-darwin-x64.tar.gz"
      sha256 "0abb243ca18c58a2a3af0abd0a0dda92f5e74eb28834ecf7d46d6f12a36cb2f7"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/bivvy-dev/bivvy/releases/download/#{version}/bivvy-linux-arm64.tar.gz"
      sha256 "5b284b1b1f9f66c017dcbb42b1e471159872283a85b313801e0dd613d33cf55a"
    end
    on_intel do
      url "https://github.com/bivvy-dev/bivvy/releases/download/#{version}/bivvy-linux-x64.tar.gz"
      sha256 "484eb6af4f39ad7b9bba901530f03ff0f4b15f4280c45a7d596e4ddd56fa60fa"
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
