use super::{Confidence, ErrorPattern, FixTemplate, PatternContext};

lazy_regex!(
    RE_BUNDLER_NATIVE_EXT,
    r"error occurred while installing (\S+)"
);
lazy_regex!(
    RE_BUNDLER_VERSION_CONFLICT,
    r"Bundler could not find compatible versions"
);
lazy_regex!(RE_BUNDLER_GEM_NOT_FOUND, r"Could not find gem '([^']+)'");
lazy_regex!(
    RE_BUNDLER_VERSION_MISSING,
    r"Could not find 'bundler' \(([^)]+)\)"
);
lazy_regex!(
    RE_RUBY_VERSION_MISMATCH,
    r"Your Ruby version is .+ but your Gemfile specified"
);

pub fn patterns() -> Vec<ErrorPattern> {
    vec![
        ErrorPattern {
            name: "bundler_native_ext",
            regex: RE_BUNDLER_NATIVE_EXT.as_str(),
            context: PatternContext::CommandContains("bundle"),
            confidence: Confidence::High,
            fix: FixTemplate::Template {
                label: "bundle update {1}",
                command: "bundle update {1}",
                explanation: "{1} failed to build native extensions",
            },
        },
        ErrorPattern {
            name: "bundler_version_conflict",
            regex: RE_BUNDLER_VERSION_CONFLICT.as_str(),
            context: PatternContext::CommandContains("bundle"),
            confidence: Confidence::High,
            fix: FixTemplate::Static {
                label: "bundle update",
                command: "bundle update",
                explanation: "Bundler version conflict detected",
            },
        },
        ErrorPattern {
            name: "bundler_gem_not_found",
            regex: RE_BUNDLER_GEM_NOT_FOUND.as_str(),
            context: PatternContext::CommandContains("bundle"),
            confidence: Confidence::High,
            fix: FixTemplate::Static {
                label: "bundle install",
                command: "bundle install",
                explanation: "Required gem not found",
            },
        },
        ErrorPattern {
            name: "bundler_version_missing",
            regex: RE_BUNDLER_VERSION_MISSING.as_str(),
            context: PatternContext::CommandContains("bundle"),
            confidence: Confidence::High,
            fix: FixTemplate::Template {
                label: "gem install bundler:{1}",
                command: "gem install bundler:{1}",
                explanation: "Bundler {1} is required by Gemfile.lock but not installed",
            },
        },
        ErrorPattern {
            name: "ruby_version_mismatch",
            regex: RE_RUBY_VERSION_MISMATCH.as_str(),
            context: PatternContext::CommandContains("bundle"),
            confidence: Confidence::Low,
            fix: FixTemplate::Hint {
                label: "check .ruby-version",
                explanation: "Ruby version doesn't match Gemfile requirement",
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use test_helpers::*;

    #[test]
    fn bundler_native_ext_matches() {
        let ctx = bundle_context();
        let error = "An error occurred while installing nokogiri (1.14.0)";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "bundle update nokogiri");
    }

    #[test]
    fn bundler_native_ext_extracts_gem_name() {
        let ctx = bundle_context();
        let error = "An error occurred while installing pg (1.5.0)";
        let fix = find_fix(error, &ctx).unwrap();
        assert!(fix.command.contains("pg"));
        assert!(fix.label.contains("pg"));
    }

    #[test]
    fn bundler_version_conflict_matches() {
        let ctx = bundle_context();
        let error = "Bundler could not find compatible versions for gem \"rails\"";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "bundle update");
    }

    #[test]
    fn bundler_version_missing_matches() {
        let ctx = bundle_context();
        let error = "Could not find 'bundler' (4.0.9) required by your /path/to/Gemfile.lock.";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "gem install bundler:4.0.9");
    }

    #[test]
    fn bundler_version_missing_extracts_version() {
        let ctx = bundle_context();
        let error = "Could not find 'bundler' (2.5.6) required by your Gemfile.lock.";
        let fix = find_fix(error, &ctx).unwrap();
        assert!(fix.command.contains("2.5.6"));
        assert!(fix.label.contains("2.5.6"));
        assert!(fix.explanation.contains("2.5.6"));
    }

    #[test]
    fn ruby_version_mismatch_returns_hint() {
        let ctx = bundle_context();
        let error = "Your Ruby version is 3.2.0 but your Gemfile specified 3.3.0";
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains(".ruby-version"));
    }
}
