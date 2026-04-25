use super::{Confidence, ErrorPattern, FixTemplate, PatternContext};

lazy_regex!(RE_RAILS_MIGRATIONS_PENDING, r"Migrations are pending");
lazy_regex!(
    RE_RAILS_NO_DATABASE,
    r"ActiveRecord::NoDatabaseError|database .* does not exist"
);
lazy_regex!(
    RE_RAILS_RELATION_NOT_EXIST,
    r#"relation "([^"]+)" does not exist"#
);

pub fn patterns() -> Vec<ErrorPattern> {
    vec![
        ErrorPattern {
            name: "rails_migrations_pending",
            regex: RE_RAILS_MIGRATIONS_PENDING.as_str(),
            context: PatternContext::CommandContains("rails|rake"),
            confidence: Confidence::High,
            fix: FixTemplate::Static {
                label: "bundle exec rails db:migrate",
                command: "bundle exec rails db:migrate",
                explanation: "Database migrations are pending",
            },
        },
        ErrorPattern {
            name: "rails_no_database",
            regex: RE_RAILS_NO_DATABASE.as_str(),
            context: PatternContext::CommandContains("rails|rake"),
            confidence: Confidence::High,
            fix: FixTemplate::Static {
                label: "bundle exec rails db:create",
                command: "bundle exec rails db:create",
                explanation: "Database has not been created",
            },
        },
        ErrorPattern {
            name: "rails_relation_not_exist",
            regex: RE_RAILS_RELATION_NOT_EXIST.as_str(),
            context: PatternContext::CommandContains("rails|rake"),
            confidence: Confidence::High,
            fix: FixTemplate::Static {
                label: "bundle exec rails db:schema:load",
                command: "bundle exec rails db:schema:load",
                explanation: "Database schema is out of date",
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn rails_migrations_pending_matches() {
        let ctx = StepContext {
            name: "server",
            command: "bundle exec rails server",
            requires: &[],
            template: None,
        };
        let error = "Migrations are pending. To resolve this issue, run:\n  bin/rails db:migrate";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "bundle exec rails db:migrate");
    }

    #[test]
    fn rails_migrations_pending_rake_context() {
        let ctx = StepContext {
            name: "test",
            command: "bundle exec rake spec",
            requires: &[],
            template: None,
        };
        let error = "Migrations are pending";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "bundle exec rails db:migrate");
    }

    #[test]
    fn rails_no_database_matches() {
        let ctx = StepContext {
            name: "db",
            command: "bundle exec rails db:migrate",
            requires: &[],
            template: None,
        };
        let error = "ActiveRecord::NoDatabaseError: could not connect";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "bundle exec rails db:create");
    }

    #[test]
    fn rails_relation_not_exist_matches() {
        let ctx = StepContext {
            name: "test",
            command: "bundle exec rails test",
            requires: &[],
            template: None,
        };
        let error = "PG::UndefinedTable: ERROR:  relation \"users\" does not exist";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "bundle exec rails db:schema:load");
    }
}
