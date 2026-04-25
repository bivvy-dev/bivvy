use super::{Confidence, ErrorPattern, FixTemplate, PatternContext};

lazy_regex!(RE_MIX_HEX_NOT_FOUND, r"Could not find Hex");
lazy_regex!(RE_MIX_REBAR_NOT_FOUND, r#"Could not find "rebar3""#);

pub fn patterns() -> Vec<ErrorPattern> {
    vec![
        ErrorPattern {
            name: "mix_hex_not_found",
            regex: RE_MIX_HEX_NOT_FOUND.as_str(),
            context: PatternContext::CommandContains("mix"),
            confidence: Confidence::High,
            fix: FixTemplate::Static {
                label: "mix local.hex --force",
                command: "mix local.hex --force",
                explanation: "Hex package manager not installed",
            },
        },
        ErrorPattern {
            name: "mix_rebar_not_found",
            regex: RE_MIX_REBAR_NOT_FOUND.as_str(),
            context: PatternContext::CommandContains("mix"),
            confidence: Confidence::High,
            fix: FixTemplate::Static {
                label: "mix local.rebar --force",
                command: "mix local.rebar --force",
                explanation: "rebar3 build tool not installed",
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::super::*;

    fn mix_context() -> StepContext<'static> {
        StepContext {
            name: "deps",
            command: "mix deps.get",
            requires: &[],
            template: None,
        }
    }

    #[test]
    fn mix_hex_not_found_matches() {
        let ctx = mix_context();
        let error = "Could not find Hex, which is needed to build dependency :phoenix";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "mix local.hex --force");
        assert_eq!(fix.explanation, "Hex package manager not installed");
    }

    #[test]
    fn mix_rebar_not_found_matches() {
        let ctx = mix_context();
        let error = r#"Could not find "rebar3", which is needed to build dependency :telemetry"#;
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "mix local.rebar --force");
        assert_eq!(fix.explanation, "rebar3 build tool not installed");
    }

    #[test]
    fn mix_pattern_wrong_context() {
        let ctx = StepContext {
            name: "build",
            command: "cargo build",
            requires: &[],
            template: None,
        };
        let error = "Could not find Hex, which is needed to build dependency :phoenix";
        assert!(find_fix(error, &ctx).is_none());
    }
}
