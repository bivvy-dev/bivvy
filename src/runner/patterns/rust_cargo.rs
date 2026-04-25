use super::{Confidence, ErrorPattern, FixTemplate, PatternContext};

lazy_regex!(
    RE_CARGO_LINKER_NOT_FOUND,
    r"linker .cc. not found|error: linker"
);
lazy_regex!(
    RE_CARGO_PKG_CONFIG,
    r"failed to run custom build command[\s\S]*?pkg.config|pkg-config.*not found"
);
lazy_regex!(
    RE_CARGO_LOCK_NEEDS_UPDATE,
    r"the lock file needs to be updated"
);
lazy_regex!(
    RE_CARGO_TOOLCHAIN_MISSING,
    r"toolchain '([^']+)' is not installed"
);

pub fn patterns() -> Vec<ErrorPattern> {
    vec![
        ErrorPattern {
            name: "cargo_linker_not_found",
            regex: RE_CARGO_LINKER_NOT_FOUND.as_str(),
            context: PatternContext::CommandContains("cargo"),
            confidence: Confidence::High,
            fix: FixTemplate::PlatformAware {
                macos_label: "xcode-select --install",
                macos_command: "xcode-select --install",
                linux_label: "sudo apt install build-essential",
                linux_command: "sudo apt install build-essential",
                explanation: "C linker not found",
            },
        },
        ErrorPattern {
            name: "cargo_pkg_config",
            regex: RE_CARGO_PKG_CONFIG.as_str(),
            context: PatternContext::CommandContains("cargo"),
            confidence: Confidence::Low,
            fix: FixTemplate::Hint {
                label: "install the required system library (e.g., libssl-dev)",
                explanation: "Missing system library for native dependency",
            },
        },
        ErrorPattern {
            name: "cargo_lock_needs_update",
            regex: RE_CARGO_LOCK_NEEDS_UPDATE.as_str(),
            context: PatternContext::CommandContains("cargo"),
            confidence: Confidence::High,
            fix: FixTemplate::Static {
                label: "cargo update",
                command: "cargo update",
                explanation: "Cargo.lock is out of date",
            },
        },
        ErrorPattern {
            name: "cargo_toolchain_missing",
            regex: RE_CARGO_TOOLCHAIN_MISSING.as_str(),
            context: PatternContext::CommandContains("cargo|rustup"),
            confidence: Confidence::High,
            fix: FixTemplate::Template {
                label: "rustup install {1}",
                command: "rustup install {1}",
                explanation: "Rust toolchain not installed",
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::super::*;

    fn cargo_context() -> StepContext<'static> {
        StepContext {
            name: "build",
            command: "cargo build",
            requires: &[],
            template: None,
        }
    }

    #[test]
    fn cargo_linker_not_found_matches() {
        let ctx = cargo_context();
        let error = "error: linker 'cc' not found";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.explanation, "C linker not found");
        if cfg!(target_os = "macos") {
            assert_eq!(fix.command, "xcode-select --install");
        } else {
            assert_eq!(fix.command, "sudo apt install build-essential");
        }
    }

    #[test]
    fn cargo_linker_not_found_alternate_message() {
        let ctx = cargo_context();
        let error = "error: linker `cc` not found\n  |\n  = note: No such file or directory";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.explanation, "C linker not found");
    }

    #[test]
    fn cargo_linker_not_found_wrong_context() {
        let ctx = StepContext {
            name: "build",
            command: "make build",
            requires: &[],
            template: None,
        };
        let error = "error: linker 'cc' not found";
        assert!(find_fix(error, &ctx).is_none());
    }

    #[test]
    fn cargo_pkg_config_returns_hint() {
        let ctx = cargo_context();
        let error = "pkg-config was not found on the system, not found";
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains("system library"));
    }

    #[test]
    fn cargo_pkg_config_build_command_variant() {
        let ctx = cargo_context();
        let error =
            "error: failed to run custom build command for `openssl-sys`\npkg-config not found";
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains("system library"));
    }

    #[test]
    fn cargo_lock_needs_update_matches() {
        let ctx = cargo_context();
        let error = "error: the lock file needs to be updated but --locked was passed";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "cargo update");
        assert_eq!(fix.explanation, "Cargo.lock is out of date");
    }

    #[test]
    fn cargo_toolchain_missing_matches() {
        let ctx = StepContext {
            name: "build",
            command: "rustup run nightly cargo build",
            requires: &[],
            template: None,
        };
        let error = "error: toolchain 'nightly-2024-01-01' is not installed";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "rustup install nightly-2024-01-01");
        assert_eq!(fix.explanation, "Rust toolchain not installed");
    }

    #[test]
    fn cargo_toolchain_missing_extracts_name() {
        let ctx = cargo_context();
        let error = "error: toolchain 'stable-x86_64-unknown-linux-gnu' is not installed";
        let fix = find_fix(error, &ctx).unwrap();
        assert!(fix.command.contains("stable-x86_64-unknown-linux-gnu"));
        assert!(fix.label.contains("stable-x86_64-unknown-linux-gnu"));
    }
}
