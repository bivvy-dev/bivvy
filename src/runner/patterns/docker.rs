use super::{Confidence, ErrorPattern, FixTemplate, PatternContext};

lazy_regex!(RE_DOCKER_DAEMON, r"Cannot connect to the Docker daemon");
lazy_regex!(
    RE_DOCKER_PORT_CONFLICT,
    r"port is already allocated|Bind for .+:\d+ failed|address already in use"
);
lazy_regex!(
    RE_DOCKER_NETWORK_NOT_FOUND,
    r"network (\S+) (?:not found|was not found)"
);
lazy_regex!(
    RE_DOCKER_IMAGE_NOT_FOUND,
    r"pull access denied|repository does not exist|manifest unknown"
);

pub fn patterns() -> Vec<ErrorPattern> {
    vec![
        ErrorPattern {
            name: "docker_daemon",
            regex: RE_DOCKER_DAEMON.as_str(),
            context: PatternContext::Always,
            confidence: Confidence::High,
            fix: FixTemplate::PlatformAware {
                macos_label: "open -a Docker",
                macos_command: "open -a Docker",
                linux_label: "systemctl start docker",
                linux_command: "systemctl start docker",
                explanation: "Docker daemon is not running",
            },
        },
        ErrorPattern {
            name: "docker_port_conflict",
            regex: RE_DOCKER_PORT_CONFLICT.as_str(),
            context: PatternContext::CommandContains("docker"),
            confidence: Confidence::High,
            fix: FixTemplate::Static {
                label: "docker compose down (also try lsof -i :PORT)",
                command: "docker compose down",
                explanation: "Port conflict \u{2014} another process is using the port",
            },
        },
        ErrorPattern {
            name: "docker_network_not_found",
            regex: RE_DOCKER_NETWORK_NOT_FOUND.as_str(),
            context: PatternContext::CommandContains("docker"),
            confidence: Confidence::High,
            fix: FixTemplate::Template {
                label: "docker network create {1}",
                command: "docker network create {1}",
                explanation: "Docker network does not exist",
            },
        },
        ErrorPattern {
            name: "docker_image_not_found",
            regex: RE_DOCKER_IMAGE_NOT_FOUND.as_str(),
            context: PatternContext::CommandContains("docker"),
            confidence: Confidence::Low,
            fix: FixTemplate::Hint {
                label: "check image name or login to registry",
                explanation: "Docker image not found or access denied",
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::super::*;

    fn docker_context() -> StepContext<'static> {
        StepContext {
            name: "docker",
            command: "docker compose up",
            requires: &[],
            template: None,
        }
    }

    #[test]
    fn docker_daemon_matches_any_step() {
        let ctx = test_helpers::default_context();
        let error = "Cannot connect to the Docker daemon at unix:///var/run/docker.sock";
        let fix = find_fix(error, &ctx).unwrap();
        assert!(fix.command.contains("docker") || fix.command.contains("Docker"));
    }

    #[test]
    fn docker_port_conflict_matches() {
        let ctx = docker_context();
        let error = "Error response from daemon: driver failed: port is already allocated";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "docker compose down");
        assert!(fix.label.contains("lsof"));
    }

    #[test]
    fn docker_port_conflict_bind_variant() {
        let ctx = docker_context();
        let error = "Bind for 0.0.0.0:3000 failed: port is already allocated";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "docker compose down");
    }

    #[test]
    fn docker_network_not_found_matches() {
        let ctx = docker_context();
        let error = "network my_app_network not found";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "docker network create my_app_network");
    }

    #[test]
    fn docker_network_was_not_found_variant() {
        let ctx = docker_context();
        let error = "network backend_net was not found";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "docker network create backend_net");
    }

    #[test]
    fn docker_image_not_found_returns_hint() {
        let ctx = StepContext {
            name: "docker",
            command: "docker pull myregistry/myimage",
            requires: &[],
            template: None,
        };
        let error = "pull access denied for myregistry/myimage, repository does not exist";
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains("image name"));
    }

    #[test]
    fn docker_port_conflict_requires_docker_context() {
        let ctx = StepContext {
            name: "server",
            command: "rails server",
            requires: &[],
            template: None,
        };
        let error = "port is already allocated";
        assert!(find_fix(error, &ctx).is_none());
    }
}
