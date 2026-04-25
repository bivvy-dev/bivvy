use super::{Confidence, ErrorPattern, FixTemplate, PatternContext};

lazy_regex!(RE_COMMAND_NOT_FOUND, r"command not found: (\S+)");
lazy_regex!(RE_PERMISSION_DENIED, r"Permission denied");
lazy_regex!(
    RE_SSL_CERTIFICATE_ERROR,
    r"SSL certificate problem|certificate verify failed|CERTIFICATE_VERIFY_FAILED"
);
lazy_regex!(
    RE_GIT_SSH_PERMISSION_DENIED,
    r"Permission denied \(publickey\)|Could not read from remote repository"
);

pub fn patterns() -> Vec<ErrorPattern> {
    vec![
        ErrorPattern {
            name: "ssl_certificate_error",
            regex: RE_SSL_CERTIFICATE_ERROR.as_str(),
            context: PatternContext::Always,
            confidence: Confidence::Low,
            fix: FixTemplate::Hint {
                label: "check SSL certificates or set SSL_CERT_FILE",
                explanation: "SSL certificate verification failed",
            },
        },
        ErrorPattern {
            name: "git_ssh_permission_denied",
            regex: RE_GIT_SSH_PERMISSION_DENIED.as_str(),
            context: PatternContext::CommandContains("git"),
            confidence: Confidence::Low,
            fix: FixTemplate::Hint {
                label: "check SSH keys with `ssh -T git@github.com`",
                explanation: "Git SSH authentication failed",
            },
        },
        ErrorPattern {
            name: "command_not_found",
            regex: RE_COMMAND_NOT_FOUND.as_str(),
            context: PatternContext::Always,
            confidence: Confidence::Low,
            fix: FixTemplate::Hint {
                label: "install {1}",
                explanation: "'{1}' is not installed",
            },
        },
        ErrorPattern {
            name: "permission_denied",
            regex: RE_PERMISSION_DENIED.as_str(),
            context: PatternContext::Always,
            confidence: Confidence::Low,
            fix: FixTemplate::Hint {
                label: "check file permissions",
                explanation: "Permission denied",
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn permission_denied_returns_hint() {
        let ctx = test_helpers::default_context();
        let error = "Permission denied (os error 13)";
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains("permissions"));
    }

    #[test]
    fn ssl_certificate_error_returns_hint() {
        let ctx = test_helpers::default_context();
        let error = "SSL certificate problem: unable to get local issuer certificate";
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains("SSL"));
    }

    #[test]
    fn ssl_certificate_verify_failed_variant() {
        let ctx = test_helpers::default_context();
        let error = "certificate verify failed (OpenSSL::SSL::SSLError)";
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains("SSL"));
    }

    #[test]
    fn git_ssh_permission_denied_returns_hint() {
        let ctx = StepContext {
            name: "clone",
            command: "git clone git@github.com:user/repo.git",
            requires: &[],
            template: None,
        };
        let error = "fatal: Could not read from remote repository.";
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains("SSH"));
    }

    #[test]
    fn git_ssh_requires_git_context() {
        let ctx = StepContext {
            name: "build",
            command: "cargo build",
            requires: &[],
            template: None,
        };
        let error = "fatal: Could not read from remote repository.";
        assert!(find_hint(error, &ctx).is_none());
    }
}
