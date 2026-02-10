# frozen_string_literal: true

# Downloads the bivvy binary during gem installation

require "fileutils"
require "net/http"
require "uri"
require "rubygems/package"
require "zlib"
require "stringio"

VERSION = File.read(File.expand_path("../../VERSION", __dir__)).strip rescue "0.0.0"
GITHUB_REPO = "bivvy-dev/bivvy"

def platform
  os = case RbConfig::CONFIG["host_os"]
       when /darwin/i then "darwin"
       when /linux/i then "linux"
       when /mswin|mingw|cygwin/i then "windows"
       else raise "Unsupported OS: #{RbConfig::CONFIG["host_os"]}"
       end

  arch = case RbConfig::CONFIG["host_cpu"]
         when /x86_64|amd64/i then "x64"
         when /arm64|aarch64/i then "arm64"
         else raise "Unsupported architecture: #{RbConfig::CONFIG["host_cpu"]}"
         end

  "#{os}-#{arch}"
end

def download_binary
  url = "https://github.com/#{GITHUB_REPO}/releases/download/#{VERSION}/bivvy-#{platform}.tar.gz"
  puts "Downloading bivvy from #{url}"

  uri = URI(url)
  response = Net::HTTP.get_response(uri)

  # Follow redirects
  while response.is_a?(Net::HTTPRedirection)
    uri = URI(response["location"])
    response = Net::HTTP.get_response(uri)
  end

  raise "Download failed: #{response.code}" unless response.is_a?(Net::HTTPSuccess)

  # Extract tar.gz
  bin_dir = File.expand_path("../../exe", __dir__)
  FileUtils.mkdir_p(bin_dir)

  Gem::Package::TarReader.new(Zlib::GzipReader.new(StringIO.new(response.body))) do |tar|
    tar.each do |entry|
      if entry.file? && entry.full_name == "bivvy"
        File.open(File.join(bin_dir, "bivvy-bin"), "wb") do |f|
          f.write(entry.read)
        end
        FileUtils.chmod(0o755, File.join(bin_dir, "bivvy-bin"))
      end
    end
  end

  puts "bivvy installed successfully"
end

download_binary

# Create empty Makefile (required by extconf.rb contract)
File.write("Makefile", "install:\n\t@echo 'Nothing to compile'\n")
