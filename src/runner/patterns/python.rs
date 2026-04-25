use super::{Confidence, ErrorPattern, FixTemplate, PatternContext};

lazy_regex!(
    RE_PIP_MODULE_NOT_FOUND,
    r"ModuleNotFoundError: No module named '([^']+)'"
);
lazy_regex!(
    RE_PYTHON_EXTERNALLY_MANAGED,
    r"externally-managed-environment|This environment is externally managed"
);
lazy_regex!(
    RE_PIP_NO_MATCHING_DISTRIBUTION,
    r"No matching distribution found for (\S+)"
);
lazy_regex!(
    RE_POETRY_LOCK_INCONSISTENT,
    r"poetry\.lock is not consistent with pyproject\.toml"
);
lazy_regex!(RE_PYTHON_NO_VENV_MODULE, r"No module named 'venv'");

pub fn patterns() -> Vec<ErrorPattern> {
    vec![
        ErrorPattern {
            name: "pip_module_not_found",
            regex: RE_PIP_MODULE_NOT_FOUND.as_str(),
            context: PatternContext::CommandContains("pip|python"),
            confidence: Confidence::High,
            fix: FixTemplate::Template {
                label: "pip install {1}",
                command: "pip install {1}",
                explanation: "Python module '{1}' not found",
            },
        },
        ErrorPattern {
            name: "python_externally_managed",
            regex: RE_PYTHON_EXTERNALLY_MANAGED.as_str(),
            context: PatternContext::CommandContains("pip|python"),
            confidence: Confidence::High,
            fix: FixTemplate::Static {
                label: "python -m venv .venv && source .venv/bin/activate",
                command: "python -m venv .venv && source .venv/bin/activate",
                explanation: "System Python is externally managed (PEP 668)",
            },
        },
        ErrorPattern {
            name: "pip_no_matching_distribution",
            regex: RE_PIP_NO_MATCHING_DISTRIBUTION.as_str(),
            context: PatternContext::CommandContains("pip|python"),
            confidence: Confidence::Low,
            fix: FixTemplate::Hint {
                label: "check Python version compatibility or package name",
                explanation: "No matching distribution found",
            },
        },
        ErrorPattern {
            name: "poetry_lock_inconsistent",
            regex: RE_POETRY_LOCK_INCONSISTENT.as_str(),
            context: PatternContext::CommandContains("poetry"),
            confidence: Confidence::High,
            fix: FixTemplate::Static {
                label: "poetry lock",
                command: "poetry lock",
                explanation: "poetry.lock is out of sync with pyproject.toml",
            },
        },
        ErrorPattern {
            name: "python_no_venv_module",
            regex: RE_PYTHON_NO_VENV_MODULE.as_str(),
            context: PatternContext::CommandContains("pip|python"),
            confidence: Confidence::High,
            fix: FixTemplate::PlatformAware {
                macos_label: "python3 -m ensurepip",
                macos_command: "python3 -m ensurepip",
                linux_label: "sudo apt install python3-venv",
                linux_command: "sudo apt install python3-venv",
                explanation: "Python venv module not installed",
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn pip_module_not_found_matches() {
        let ctx = StepContext {
            name: "deps",
            command: "pip install -r requirements.txt",
            requires: &[],
            template: None,
        };
        let error = "ModuleNotFoundError: No module named 'flask'";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "pip install flask");
    }

    #[test]
    fn python_externally_managed_matches() {
        let ctx = StepContext {
            name: "deps",
            command: "pip install flask",
            requires: &[],
            template: None,
        };
        let error =
            "error: externally-managed-environment\n\n× This environment is externally managed";
        let fix = find_fix(error, &ctx).unwrap();
        assert!(fix.command.contains("venv"));
        assert_eq!(
            fix.explanation,
            "System Python is externally managed (PEP 668)"
        );
    }

    #[test]
    fn python_externally_managed_alternate_message() {
        let ctx = StepContext {
            name: "deps",
            command: "python -m pip install requests",
            requires: &[],
            template: None,
        };
        let error = "This environment is externally managed";
        let fix = find_fix(error, &ctx).unwrap();
        assert!(fix.command.contains("venv"));
    }

    #[test]
    fn pip_no_matching_distribution_returns_hint() {
        let ctx = StepContext {
            name: "deps",
            command: "pip install oldpackage",
            requires: &[],
            template: None,
        };
        let error = "ERROR: No matching distribution found for oldpackage==9.9.9";
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains("Python version compatibility"));
    }

    #[test]
    fn poetry_lock_inconsistent_matches() {
        let ctx = StepContext {
            name: "deps",
            command: "poetry install",
            requires: &[],
            template: None,
        };
        let error = "Warning: poetry.lock is not consistent with pyproject.toml. You may be getting improper dependencies.";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "poetry lock");
        assert_eq!(
            fix.explanation,
            "poetry.lock is out of sync with pyproject.toml"
        );
    }

    #[test]
    fn python_no_venv_module_matches() {
        let ctx = StepContext {
            name: "venv",
            command: "python -m venv .venv",
            requires: &[],
            template: None,
        };
        let error = "Error: No module named 'venv'";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.explanation, "Python venv module not installed");
        if cfg!(target_os = "macos") {
            assert!(fix.command.contains("ensurepip"));
        } else {
            assert!(fix.command.contains("apt install python3-venv"));
        }
    }
}
