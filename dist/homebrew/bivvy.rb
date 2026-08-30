# frozen_string_literal: true

class Bivvy < Formula
  desc "Cross-language development environment setup automation"
  homepage "https://bivvy.dev"
  version "1.11.0"
  license "FSL-1.1-Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/bivvy-dev/bivvy/releases/download/#{version}/bivvy-darwin-arm64.tar.gz"
      sha256 "db27a90d62795e00c8a83abb51fc5f6947203f6f0d9a1ea70cf591cfb91d50b9"
    end
    on_intel do
      url "https://github.com/bivvy-dev/bivvy/releases/download/#{version}/bivvy-darwin-x64.tar.gz"
      sha256 "d7a12eb3c5dad946b16d2dbeb9e383bd8fbc12bdf3791861a11bef9a2e8a88bd"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/bivvy-dev/bivvy/releases/download/#{version}/bivvy-linux-arm64.tar.gz"
      sha256 "30953824235d0021fe43ee2394c8edf74a1a5dd40505afae8aafe37a4d0cb09e"
    end
    on_intel do
      url "https://github.com/bivvy-dev/bivvy/releases/download/#{version}/bivvy-linux-x64.tar.gz"
      sha256 "6e1e278168ece46e552807620e65442bb98c610e67f964258a80f656638d4f90"
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
