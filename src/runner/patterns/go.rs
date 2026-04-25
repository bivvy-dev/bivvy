use super::{Confidence, ErrorPattern, FixTemplate, PatternContext};

lazy_regex!(RE_GO_MISSING_SUM, r"missing go\.sum entry for module");
lazy_regex!(RE_GO_CHECKSUM_MISMATCH, r"checksum mismatch");

pub fn patterns() -> Vec<ErrorPattern> {
    vec![
        ErrorPattern {
            name: "go_missing_sum",
            regex: RE_GO_MISSING_SUM.as_str(),
            context: PatternContext::CommandContains(
                "go build|go run|go test|go mod|go get|go install|go clean|go vet",
            ),
            confidence: Confidence::High,
            fix: FixTemplate::Static {
                label: "go mod tidy",
                command: "go mod tidy",
                explanation: "go.sum is missing module entries",
            },
        },
        ErrorPattern {
            name: "go_checksum_mismatch",
            regex: RE_GO_CHECKSUM_MISMATCH.as_str(),
            context: PatternContext::CommandContains(
                "go build|go run|go test|go mod|go get|go install|go clean|go vet",
            ),
            confidence: Confidence::High,
            fix: FixTemplate::Static {
                label: "go clean -modcache && go mod download",
                command: "go clean -modcache && go mod download",
                explanation: "Go module checksum mismatch",
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::super::*;

    fn go_context() -> StepContext<'static> {
        StepContext {
            name: "build",
            command: "go build ./...",
            requires: &[],
            template: None,
        }
    }

    #[test]
    fn go_missing_sum_matches() {
        let ctx = go_context();
        let error = "missing go.sum entry for module providing package github.com/foo/bar";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "go mod tidy");
        assert_eq!(fix.explanation, "go.sum is missing module entries");
    }

    #[test]
    fn go_checksum_mismatch_matches() {
        let ctx = go_context();
        let error = "verifying github.com/foo/bar@v1.0.0: checksum mismatch";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "go clean -modcache && go mod download");
        assert_eq!(fix.explanation, "Go module checksum mismatch");
    }

    #[test]
    fn go_pattern_requires_go_context() {
        let ctx = StepContext {
            name: "build",
            command: "make build",
            requires: &[],
            template: None,
        };
        let error = "missing go.sum entry for module providing package github.com/foo/bar";
        assert!(find_fix(error, &ctx).is_none());
    }
}
