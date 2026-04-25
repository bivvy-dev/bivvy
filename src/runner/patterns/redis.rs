use super::{Confidence, ErrorPattern, FixTemplate, PatternContext};

lazy_regex!(
    RE_REDIS_CONN_REFUSED,
    r"Connection refused.*6379|Error connecting to Redis"
);

pub fn patterns() -> Vec<ErrorPattern> {
    vec![ErrorPattern {
        name: "redis_conn_refused",
        regex: RE_REDIS_CONN_REFUSED.as_str(),
        context: PatternContext::RequiresAny(&["redis-server"]),
        confidence: Confidence::High,
        fix: FixTemplate::PlatformAware {
            macos_label: "brew services start redis",
            macos_command: "brew services start redis",
            linux_label: "systemctl start redis",
            linux_command: "systemctl start redis",
            explanation: "Redis server is not running",
        },
    }]
}
