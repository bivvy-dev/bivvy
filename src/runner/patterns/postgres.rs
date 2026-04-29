use super::{Confidence, ErrorPattern, FixTemplate, PatternContext};

// Matches PostgreSQL connection refused errors indicating the server is not running.
lazy_regex!(
    RE_POSTGRES_CONN_REFUSED,
    r"could not connect to server.*Is the server running"
);
// Matches `FATAL: database "..." does not exist` errors, capturing the database name.
lazy_regex!(
    RE_POSTGRES_DB_NOT_EXIST,
    r#"FATAL:.*database "([^"]+)" does not exist"#
);
// Matches `FATAL: role "..." does not exist` errors, capturing the role name.
lazy_regex!(
    RE_POSTGRES_ROLE_NOT_EXIST,
    r#"FATAL:.*role "([^"]+)" does not exist"#
);
// Matches `pg_dump` version mismatch errors when the client and server versions differ.
lazy_regex!(
    RE_POSTGRES_PGDUMP_VERSION_MISMATCH,
    r"pg_dump: error:.*aborting because of server version mismatch"
);

/// Return error patterns for the PostgreSQL ecosystem.
///
/// Covers connection failures, missing databases, missing roles, and
/// `pg_dump` client/server version mismatches.
pub fn patterns() -> Vec<ErrorPattern> {
    vec![
        ErrorPattern {
            name: "postgres_conn_refused",
            regex: RE_POSTGRES_CONN_REFUSED.as_str(),
            context: PatternContext::RequiresAny(&["postgres-server"]),
            confidence: Confidence::High,
            fix: FixTemplate::PlatformAware {
                macos_label: "brew services start postgresql",
                macos_command: "brew services start postgresql",
                linux_label: "systemctl start postgresql",
                linux_command: "systemctl start postgresql",
                explanation: "PostgreSQL server is not running",
            },
        },
        ErrorPattern {
            name: "postgres_db_not_exist",
            regex: RE_POSTGRES_DB_NOT_EXIST.as_str(),
            context: PatternContext::RequiresAny(&["postgres-server"]),
            confidence: Confidence::High,
            fix: FixTemplate::Template {
                label: "createdb {1}",
                command: "createdb {1}",
                explanation: "Database '{1}' does not exist",
            },
        },
        ErrorPattern {
            name: "postgres_role_not_exist",
            regex: RE_POSTGRES_ROLE_NOT_EXIST.as_str(),
            context: PatternContext::RequiresAny(&["postgres-server"]),
            confidence: Confidence::High,
            fix: FixTemplate::Template {
                label: "createuser {1}",
                command: "createuser {1}",
                explanation: "PostgreSQL role '{1}' does not exist",
            },
        },
        ErrorPattern {
            name: "postgres_pgdump_version_mismatch",
            regex: RE_POSTGRES_PGDUMP_VERSION_MISMATCH.as_str(),
            context: PatternContext::Always,
            confidence: Confidence::Low,
            fix: FixTemplate::Hint {
                label: "update pg_dump to match your server version (e.g., brew install postgresql@16 and add its bin/ to PATH)",
                explanation: "pg_dump version does not match PostgreSQL server version",
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn postgres_conn_refused_matches() {
        let requires = vec!["postgres-server".to_string()];
        let ctx = StepContext {
            name: "db_setup",
            command: "rails db:create",
            requires: &requires,
            template: None,
        };
        let error = "PG::ConnectionBad: could not connect to server: Is the server running on host";
        let fix = find_fix(error, &ctx).unwrap();
        assert!(fix.command.contains("postgresql"));
    }

    #[test]
    fn postgres_db_not_exist_extracts_name() {
        let requires = vec!["postgres-server".to_string()];
        let ctx = StepContext {
            name: "db_setup",
            command: "rails db:create",
            requires: &requires,
            template: None,
        };
        let error = "FATAL:  database \"myapp_dev\" does not exist";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "createdb myapp_dev");
    }

    #[test]
    fn postgres_pgdump_version_mismatch_returns_hint() {
        let ctx = StepContext {
            name: "db_setup",
            command: "rails db:prepare",
            requires: &[],
            template: None,
        };
        let error = "pg_dump: error: server version: 16.13 (Homebrew); pg_dump version: 14.21 (Homebrew)\npg_dump: error: aborting because of server version mismatch";
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains("pg_dump"));
        assert!(hint.contains("server version"));
    }

    #[test]
    fn postgres_pgdump_version_mismatch_fires_without_requires() {
        let ctx = test_helpers::default_context();
        let error = "pg_dump: error: aborting because of server version mismatch";
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains("pg_dump"));
    }
}
