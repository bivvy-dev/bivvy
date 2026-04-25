use super::{Confidence, ErrorPattern, FixTemplate, PatternContext};

lazy_regex!(
    RE_DOTNET_FRAMEWORK_NOT_FOUND,
    r"The framework '([^']+)'.* was not found|The specified framework '([^']+)' was not found"
);

pub fn patterns() -> Vec<ErrorPattern> {
    vec![ErrorPattern {
        name: "dotnet_framework_not_found",
        regex: RE_DOTNET_FRAMEWORK_NOT_FOUND.as_str(),
        context: PatternContext::CommandContains("dotnet"),
        confidence: Confidence::Low,
        fix: FixTemplate::Hint {
            label: "run `dotnet --list-sdks` and install the required SDK",
            explanation: ".NET framework/SDK not found",
        },
    }]
}

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn dotnet_framework_not_found_returns_hint() {
        let ctx = StepContext {
            name: "build",
            command: "dotnet build",
            requires: &[],
            template: None,
        };
        let error = "The framework 'Microsoft.NETCore.App', version '7.0.0' was not found.";
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains("dotnet --list-sdks"));
    }

    #[test]
    fn dotnet_specified_framework_variant() {
        let ctx = StepContext {
            name: "run",
            command: "dotnet run",
            requires: &[],
            template: None,
        };
        let error = "The specified framework 'Microsoft.AspNetCore.App' was not found.";
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains("dotnet --list-sdks"));
    }
}
