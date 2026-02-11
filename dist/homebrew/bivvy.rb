# frozen_string_literal: true

class Bivvy < Formula
  desc "Cross-language development environment setup automation"
  homepage "https://bivvy.dev"
  version "1.4.0"
  license "FSL-1.1-Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/bivvy-dev/bivvy/releases/download/#{version}/bivvy-darwin-arm64.tar.gz"
      sha256 "f2ad7acf42f950c70bfca567200e77216329a219b40701b66c8670af59af2531"
    end
    on_intel do
      url "https://github.com/bivvy-dev/bivvy/releases/download/#{version}/bivvy-darwin-x64.tar.gz"
      sha256 "6c781ef56905d07215a5e7ac2f056c83a37ab67b83f3d7e522099aa7481d838e"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/bivvy-dev/bivvy/releases/download/#{version}/bivvy-linux-arm64.tar.gz"
      sha256 "78572680c0080cff7420c3e5bb2c4c4b0c680dda25409db8463b413b740f35f6"
    end
    on_intel do
      url "https://github.com/bivvy-dev/bivvy/releases/download/#{version}/bivvy-linux-x64.tar.gz"
      sha256 "65dfe0824c8350a0d6629495ee30ff2846bcb1383112ea403579e161718e4d69"
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
