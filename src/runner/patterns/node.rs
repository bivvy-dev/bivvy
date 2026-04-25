use super::{Confidence, ErrorPattern, FixTemplate, PatternContext};

lazy_regex!(
    RE_NPM_MODULE_NOT_FOUND,
    r"Cannot find module|MODULE_NOT_FOUND"
);
lazy_regex!(RE_YARN_INTEGRITY, r"integrity check failed");
lazy_regex!(RE_NPM_ERESOLVE, r"ERESOLVE unable to resolve");
lazy_regex!(RE_NODE_OPENSSL_UNSUPPORTED, r"ERR_OSSL_EVP_UNSUPPORTED");
lazy_regex!(
    RE_NPM_ENOSPC_WATCHERS,
    r"ENOSPC.*System limit for number of file watchers|inotify_add_watch"
);
lazy_regex!(
    RE_NODE_ENGINE_MISMATCH,
    r#"engine "node" is incompatible|The engines\.node"#
);

pub fn patterns() -> Vec<ErrorPattern> {
    vec![
        ErrorPattern {
            name: "npm_module_not_found",
            regex: RE_NPM_MODULE_NOT_FOUND.as_str(),
            context: PatternContext::CommandContains("npm"),
            confidence: Confidence::High,
            fix: FixTemplate::Static {
                label: "npm install",
                command: "npm install",
                explanation: "Node module not found",
            },
        },
        ErrorPattern {
            name: "yarn_integrity",
            regex: RE_YARN_INTEGRITY.as_str(),
            context: PatternContext::CommandContains("yarn"),
            confidence: Confidence::High,
            fix: FixTemplate::Static {
                label: "yarn install --check-files",
                command: "yarn install --check-files",
                explanation: "Yarn integrity check failed",
            },
        },
        ErrorPattern {
            name: "npm_eresolve",
            regex: RE_NPM_ERESOLVE.as_str(),
            context: PatternContext::CommandContains("npm"),
            confidence: Confidence::High,
            fix: FixTemplate::Static {
                label: "npm install --legacy-peer-deps",
                command: "npm install --legacy-peer-deps",
                explanation: "npm peer dependency conflict",
            },
        },
        ErrorPattern {
            name: "npm_enospc_watchers",
            regex: RE_NPM_ENOSPC_WATCHERS.as_str(),
            context: PatternContext::CommandContains("npm|node|yarn|npx"),
            confidence: Confidence::High,
            fix: FixTemplate::Static {
                label: "sysctl -w fs.inotify.max_user_watches=524288",
                command: "sysctl -w fs.inotify.max_user_watches=524288",
                explanation: "System file watcher limit reached",
            },
        },
        ErrorPattern {
            name: "node_openssl_unsupported",
            regex: RE_NODE_OPENSSL_UNSUPPORTED.as_str(),
            context: PatternContext::CommandContains("npm|node|yarn|npx"),
            confidence: Confidence::Low,
            fix: FixTemplate::Hint {
                label: "export NODE_OPTIONS=--openssl-legacy-provider",
                explanation: "Node.js OpenSSL provider incompatibility",
            },
        },
        ErrorPattern {
            name: "node_engine_mismatch",
            regex: RE_NODE_ENGINE_MISMATCH.as_str(),
            context: PatternContext::CommandContains("npm|yarn"),
            confidence: Confidence::Low,
            fix: FixTemplate::Hint {
                label: "check .node-version or .nvmrc",
                explanation: "Node.js version doesn't match package requirement",
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::super::*;

    fn npm_context() -> StepContext<'static> {
        StepContext {
            name: "npm_install",
            command: "npm install",
            requires: &[],
            template: None,
        }
    }

    fn node_context() -> StepContext<'static> {
        StepContext {
            name: "node_build",
            command: "node build.js",
            requires: &[],
            template: None,
        }
    }

    #[test]
    fn npm_module_not_found_matches() {
        let ctx = StepContext {
            name: "npm",
            command: "npm run build",
            requires: &[],
            template: None,
        };
        let error = "Error: Cannot find module 'webpack'";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "npm install");
    }

    #[test]
    fn npm_eresolve_matches() {
        let ctx = npm_context();
        let error = "npm ERR! ERESOLVE unable to resolve dependency tree";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "npm install --legacy-peer-deps");
        assert_eq!(fix.confidence, Confidence::High);
    }

    #[test]
    fn npm_eresolve_excludes_non_npm() {
        let ctx = node_context();
        let error = "npm ERR! ERESOLVE unable to resolve dependency tree";
        assert!(find_fix(error, &ctx).is_none());
    }

    #[test]
    fn node_openssl_returns_hint() {
        let ctx = node_context();
        let error = "Error: error:0308010C:digital envelope routines::unsupported\ncode: 'ERR_OSSL_EVP_UNSUPPORTED'";
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains("openssl-legacy-provider"));
    }

    #[test]
    fn node_openssl_matches_yarn_context() {
        let ctx = StepContext {
            name: "build",
            command: "yarn build",
            requires: &[],
            template: None,
        };
        let error = "ERR_OSSL_EVP_UNSUPPORTED";
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains("openssl-legacy-provider"));
    }

    #[test]
    fn npm_enospc_watchers_matches() {
        let ctx = npm_context();
        let error =
            "Error: ENOSPC: System limit for number of file watchers reached, watch '/app/src'";
        let fix = find_fix(error, &ctx).unwrap();
        assert!(fix.command.contains("inotify"));
        assert_eq!(fix.confidence, Confidence::High);
    }

    #[test]
    fn npm_enospc_watchers_matches_inotify() {
        let ctx = node_context();
        let error = "Error: inotify_add_watch failed";
        let fix = find_fix(error, &ctx).unwrap();
        assert!(fix.command.contains("max_user_watches"));
    }

    #[test]
    fn node_engine_mismatch_returns_hint() {
        let ctx = npm_context();
        let error = r#"error The engine "node" is incompatible with this module"#;
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains(".node-version") || hint.contains(".nvmrc"));
    }

    #[test]
    fn node_engine_mismatch_excludes_unrelated() {
        let ctx = test_helpers::default_context();
        let error = r#"error The engine "node" is incompatible with this module"#;
        assert!(find_hint(error, &ctx).is_none());
    }
}
