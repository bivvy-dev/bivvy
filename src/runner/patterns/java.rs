use super::{Confidence, ErrorPattern, FixTemplate, PatternContext};

lazy_regex!(
    RE_JAVA_HOME_NOT_SET,
    r"JAVA_HOME is not set|JAVA_HOME.*not.*defined|No java installation was detected"
);
lazy_regex!(
    RE_JAVA_VERSION_UNKNOWN,
    r"Could not determine java version|Unsupported Java version"
);
lazy_regex!(
    RE_GRADLEW_PERMISSION,
    r"gradlew.*Permission denied|mvnw.*Permission denied"
);

pub fn patterns() -> Vec<ErrorPattern> {
    vec![
        ErrorPattern {
            name: "java_home_not_set",
            regex: RE_JAVA_HOME_NOT_SET.as_str(),
            context: PatternContext::CommandContains("gradle|gradlew|mvn|maven|java"),
            confidence: Confidence::Low,
            fix: FixTemplate::Hint {
                label: "set JAVA_HOME environment variable",
                explanation: "JAVA_HOME is not configured",
            },
        },
        ErrorPattern {
            name: "java_version_unknown",
            regex: RE_JAVA_VERSION_UNKNOWN.as_str(),
            context: PatternContext::CommandContains("gradle|gradlew|mvn|maven"),
            confidence: Confidence::Low,
            fix: FixTemplate::Hint {
                label: "check Java installation",
                explanation: "Java version could not be determined",
            },
        },
        ErrorPattern {
            name: "gradlew_permission",
            regex: RE_GRADLEW_PERMISSION.as_str(),
            context: PatternContext::CommandContains("gradlew|mvnw"),
            confidence: Confidence::High,
            fix: FixTemplate::ContextSwitch {
                alternatives: &[
                    ("mvnw", "chmod +x mvnw", "chmod +x mvnw"),
                    ("gradlew", "chmod +x gradlew", "chmod +x gradlew"),
                ],
                explanation: "Build wrapper script is not executable",
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn java_home_not_set_returns_hint() {
        let ctx = StepContext {
            name: "build",
            command: "./gradlew build",
            requires: &[],
            template: None,
        };
        let error = "ERROR: JAVA_HOME is not set and no 'java' command could be found";
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains("JAVA_HOME"));
    }

    #[test]
    fn java_no_installation_detected() {
        let ctx = StepContext {
            name: "build",
            command: "gradle build",
            requires: &[],
            template: None,
        };
        let error = "No java installation was detected";
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains("JAVA_HOME"));
    }

    #[test]
    fn java_version_unknown_returns_hint() {
        let ctx = StepContext {
            name: "build",
            command: "./gradlew build",
            requires: &[],
            template: None,
        };
        let error = "Could not determine java version from '21.0.1'.";
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains("Java installation"));
    }

    #[test]
    fn gradlew_permission_denied_matches() {
        let ctx = StepContext {
            name: "build",
            command: "./gradlew build",
            requires: &[],
            template: None,
        };
        let error = "./gradlew: Permission denied";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "chmod +x gradlew");
    }

    #[test]
    fn mvnw_permission_denied_matches() {
        let ctx = StepContext {
            name: "build",
            command: "./mvnw package",
            requires: &[],
            template: None,
        };
        let error = "./mvnw: Permission denied";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "chmod +x mvnw");
    }
}
