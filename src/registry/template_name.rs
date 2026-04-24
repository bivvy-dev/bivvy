//! Typed template names for compile-time validation.
//!
//! Every template name that detection can suggest is a variant of
//! [`TemplateName`]. This makes typos a compile error instead of a
//! runtime "Unknown template" surprise.

use std::fmt;

/// A known built-in template name.
///
/// Used by the detection system to suggest templates. Each variant maps
/// to an unqualified template name in the built-in registry (e.g.
/// `TemplateName::BundleInstall` → `"bundle-install"`).
///
/// ```
/// use bivvy::registry::TemplateName;
///
/// assert_eq!(TemplateName::BundleInstall.as_str(), "bundle-install");
/// assert_eq!(TemplateName::MiseTools.as_str(), "mise-tools");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TemplateName {
    // ── System package managers ──────────────────────────────────────
    BrewBundle,
    AptInstall,
    YumInstall,
    PacmanInstall,
    ChocoInstall,

    // ── Version managers ─────────────────────────────────────────────
    MiseTools,
    AsdfTools,
    VoltaSetup,
    FnmSetup,
    NvmNode,
    RbenvRuby,
    PyenvPython,

    // ── Ruby ─────────────────────────────────────────────────────────
    BundleInstall,
    RailsDb,

    // ── Node ─────────────────────────────────────────────────────────
    YarnInstall,
    NpmInstall,
    PnpmInstall,
    BunInstall,
    NextjsBuild,
    ViteBuild,
    RemixBuild,
    PrismaMigrate,

    // ── Python ───────────────────────────────────────────────────────
    PipInstall,
    PoetryInstall,
    UvSync,
    AlembicMigrate,
    DjangoMigrate,

    // ── Rust ─────────────────────────────────────────────────────────
    CargoBuild,
    DieselMigrate,

    // ── Go ───────────────────────────────────────────────────────────
    GoModDownload,

    // ── PHP ──────────────────────────────────────────────────────────
    ComposerInstall,
    LaravelSetup,

    // ── Gradle / Kotlin ──────────────────────────────────────────────
    GradleDeps,
    SpringBootBuild,

    // ── Elixir ───────────────────────────────────────────────────────
    MixDepsGet,

    // ── Swift ────────────────────────────────────────────────────────
    SwiftResolve,

    // ── IaC ──────────────────────────────────────────────────────────
    TerraformInit,
    CdkSynth,
    PulumiInstall,
    AnsibleInstall,

    // ── Java ─────────────────────────────────────────────────────────
    MavenResolve,

    // ── .NET ─────────────────────────────────────────────────────────
    DotnetRestore,

    // ── Dart ─────────────────────────────────────────────────────────
    DartPubGet,
    FlutterPubGet,

    // ── Deno ─────────────────────────────────────────────────────────
    DenoInstall,

    // ── Containers ───────────────────────────────────────────────────
    DockerComposeUp,
    HelmDeps,

    // ── Common ───────────────────────────────────────────────────────
    EnvCopy,
    PreCommitInstall,

    // ── Monorepo ─────────────────────────────────────────────────────
    NxBuild,
    TurboBuild,
    LernaBootstrap,
}

impl TemplateName {
    /// The unqualified template name string used for registry lookups.
    pub fn as_str(&self) -> &'static str {
        match self {
            // System
            Self::BrewBundle => "brew-bundle",
            Self::AptInstall => "apt-install",
            Self::YumInstall => "yum-install",
            Self::PacmanInstall => "pacman-install",
            Self::ChocoInstall => "choco-install",
            // Version managers
            Self::MiseTools => "mise-tools",
            Self::AsdfTools => "asdf-tools",
            Self::VoltaSetup => "volta-setup",
            Self::FnmSetup => "fnm-setup",
            Self::NvmNode => "nvm-node",
            Self::RbenvRuby => "rbenv-ruby",
            Self::PyenvPython => "pyenv-python",
            // Ruby
            Self::BundleInstall => "bundle-install",
            Self::RailsDb => "rails-db",
            // Node
            Self::YarnInstall => "yarn-install",
            Self::NpmInstall => "npm-install",
            Self::PnpmInstall => "pnpm-install",
            Self::BunInstall => "bun-install",
            Self::NextjsBuild => "nextjs-build",
            Self::ViteBuild => "vite-build",
            Self::RemixBuild => "remix-build",
            Self::PrismaMigrate => "prisma-migrate",
            // Python
            Self::PipInstall => "pip-install",
            Self::PoetryInstall => "poetry-install",
            Self::UvSync => "uv-sync",
            Self::AlembicMigrate => "alembic-migrate",
            Self::DjangoMigrate => "django-migrate",
            // Rust
            Self::CargoBuild => "cargo-build",
            Self::DieselMigrate => "diesel-migrate",
            // Go
            Self::GoModDownload => "go-mod-download",
            // PHP
            Self::ComposerInstall => "composer-install",
            Self::LaravelSetup => "laravel-setup",
            // Gradle/Kotlin
            Self::GradleDeps => "gradle-deps",
            Self::SpringBootBuild => "spring-boot-build",
            // Elixir
            Self::MixDepsGet => "mix-deps-get",
            // Swift
            Self::SwiftResolve => "swift-resolve",
            // IaC
            Self::TerraformInit => "terraform-init",
            Self::CdkSynth => "cdk-synth",
            Self::PulumiInstall => "pulumi-install",
            Self::AnsibleInstall => "ansible-install",
            // Java
            Self::MavenResolve => "maven-resolve",
            // .NET
            Self::DotnetRestore => "dotnet-restore",
            // Dart
            Self::DartPubGet => "dart-pub-get",
            Self::FlutterPubGet => "flutter-pub-get",
            // Deno
            Self::DenoInstall => "deno-install",
            // Containers
            Self::DockerComposeUp => "docker-compose-up",
            Self::HelmDeps => "helm-deps",
            // Common
            Self::EnvCopy => "env-copy",
            Self::PreCommitInstall => "pre-commit-install",
            // Monorepo
            Self::NxBuild => "nx-build",
            Self::TurboBuild => "turbo-build",
            Self::LernaBootstrap => "lerna-bootstrap",
        }
    }

    /// All variants, for exhaustive validation tests.
    pub const ALL: &'static [TemplateName] = &[
        Self::BrewBundle,
        Self::AptInstall,
        Self::YumInstall,
        Self::PacmanInstall,
        Self::ChocoInstall,
        Self::MiseTools,
        Self::AsdfTools,
        Self::VoltaSetup,
        Self::FnmSetup,
        Self::NvmNode,
        Self::RbenvRuby,
        Self::PyenvPython,
        Self::BundleInstall,
        Self::RailsDb,
        Self::YarnInstall,
        Self::NpmInstall,
        Self::PnpmInstall,
        Self::BunInstall,
        Self::NextjsBuild,
        Self::ViteBuild,
        Self::RemixBuild,
        Self::PrismaMigrate,
        Self::PipInstall,
        Self::PoetryInstall,
        Self::UvSync,
        Self::AlembicMigrate,
        Self::DjangoMigrate,
        Self::CargoBuild,
        Self::DieselMigrate,
        Self::GoModDownload,
        Self::ComposerInstall,
        Self::LaravelSetup,
        Self::GradleDeps,
        Self::SpringBootBuild,
        Self::MixDepsGet,
        Self::SwiftResolve,
        Self::TerraformInit,
        Self::CdkSynth,
        Self::PulumiInstall,
        Self::AnsibleInstall,
        Self::MavenResolve,
        Self::DotnetRestore,
        Self::DartPubGet,
        Self::FlutterPubGet,
        Self::DenoInstall,
        Self::DockerComposeUp,
        Self::HelmDeps,
        Self::EnvCopy,
        Self::PreCommitInstall,
        Self::NxBuild,
        Self::TurboBuild,
        Self::LernaBootstrap,
    ];
}

impl fmt::Display for TemplateName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AsRef<str> for TemplateName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq<str> for TemplateName {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for TemplateName {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_matches_as_str() {
        for variant in TemplateName::ALL {
            assert_eq!(variant.to_string(), variant.as_str());
        }
    }

    #[test]
    fn all_array_is_exhaustive() {
        // If a variant is added to the enum but not to ALL, this test
        // won't catch it directly — but the registry validation test
        // in builtin.rs will fail because the new variant won't be
        // checked. This test just confirms ALL has no duplicates.
        let mut seen = std::collections::HashSet::new();
        for variant in TemplateName::ALL {
            assert!(
                seen.insert(variant.as_str()),
                "Duplicate in ALL: {}",
                variant
            );
        }
    }

    #[test]
    fn partial_eq_str() {
        assert!(TemplateName::BundleInstall == "bundle-install");
        assert!(TemplateName::MiseTools == "mise-tools");
        assert!(TemplateName::AsdfTools != "asdf");
    }
}
