# frozen_string_literal: true

class Bivvy < Formula
  desc "Cross-language development environment setup automation"
  homepage "https://bivvy.dev"
  version "1.3.0"
  license "FSL-1.1-Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/bivvy-dev/bivvy/releases/download/#{version}/bivvy-darwin-arm64.tar.gz"
      sha256 "02e215ce44e69a1be23eb18314855372e43ac5a08de3a8bcf36d0e207c94a37b"
    end
    on_intel do
      url "https://github.com/bivvy-dev/bivvy/releases/download/#{version}/bivvy-darwin-x64.tar.gz"
      sha256 "7b892f264ca59984ee56b314ba470cabf09db964bc4c0da5795530bad27f19f1"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/bivvy-dev/bivvy/releases/download/#{version}/bivvy-linux-arm64.tar.gz"
      sha256 "d38e499c5615b28a7fb31ecfbebd5e4ef66144fd9a0335e3470e29c517b8b4e8"
    end
    on_intel do
      url "https://github.com/bivvy-dev/bivvy/releases/download/#{version}/bivvy-linux-x64.tar.gz"
      sha256 "0ab97f5529a8d329293398bf53966c587b04db1e6918b95e7ed5c16e2e34f056"
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
