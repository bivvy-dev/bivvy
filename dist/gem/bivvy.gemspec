# frozen_string_literal: true

Gem::Specification.new do |spec|
  spec.name          = "bivvy"
  spec.version       = File.read(File.expand_path("VERSION", __dir__)).strip rescue "0.0.0"
  spec.authors       = ["Brenna Stuart"]
  spec.email         = ["support@bivvy.dev"]

  spec.summary       = "Cross-language development environment setup automation"
  spec.description   = "Bivvy replaces ad-hoc bin/setup scripts with declarative YAML configuration."
  spec.homepage      = "https://bivvy.dev"
  spec.license       = "FSL-1.1-Apache-2.0"
  spec.required_ruby_version = ">= 2.7.0"

  spec.metadata["homepage_uri"] = spec.homepage
  spec.metadata["source_code_uri"] = "https://github.com/bivvy-dev/bivvy"

  spec.files = Dir["lib/**/*", "exe/*", "ext/*", "VERSION", "LICENSE", "README.md"]
  spec.bindir = "exe"
  spec.executables = ["bivvy"]
  spec.extensions = ["ext/extconf.rb"]
end
