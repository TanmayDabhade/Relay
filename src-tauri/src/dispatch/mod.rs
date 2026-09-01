use std::path::{Path, PathBuf};

mod normalize;
mod runtime;
pub use normalize::{
    normalize_provider_line_with_state, NormalizeState, NormalizedEvent,
    NormalizedEventUpdate,
};
pub use runtime::{emit_dispatch_event, Runtime};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCommand {
    pub executable: String,
    pub args: Vec<String>,
    pub initial_stdin: Option<String>,
}

/// Builds a non-interactive, machine-readable command for one of Relay's four adapters.
/// Every provider emits JSONL on stdout; terminal presentation never crosses into the UI.
pub fn build_agent_command(
    agent: &str,
    model: &str,
    prompt: &str,
    provider_session_id: Option<&str>,
) -> anyhow::Result<AgentCommand> {
    let (executable, mut args, initial_stdin): (&str, Vec<String>, Option<String>) = match agent {
        "claude" => {
            let mut args = vec![
                "--print".to_string(),
                "--verbose".to_string(),
                "--input-format".to_string(),
                "stream-json".to_string(),
                "--output-format".to_string(),
                "stream-json".to_string(),
                "--include-partial-messages".to_string(),
                "--replay-user-messages".to_string(),
            ];
            if let Some(session_id) = provider_session_id {
                args.push("--resume".to_string());
                args.push(session_id.to_string());
            }
            let input = serde_json::json!({
                "type": "user",
                "message": {
                    "role": "user",
                    "content": [{ "type": "text", "text": prompt }]
                }
            });
            ("claude", args, Some(format!("{input}\n")))
        }
        "codex" => {
            let mut args = vec!["exec".to_string()];
            if let Some(session_id) = provider_session_id {
                args.push("resume".to_string());
                args.push(session_id.to_string());
            }
            args.push("--json".to_string());
            ("codex", args, None)
        }
        "gemini" => {
            let mut args = vec!["--output-format".to_string(), "stream-json".to_string()];
            if let Some(session_id) = provider_session_id {
                args.push("--resume".to_string());
                args.push(session_id.to_string());
            }
            ("gemini", args, None)
        }
        "cursor" => {
            let mut args = vec![
                "--print".to_string(),
                "--output-format".to_string(),
                "stream-json".to_string(),
            ];
            if let Some(session_id) = provider_session_id {
                args.push("--resume".to_string());
                args.push(session_id.to_string());
            }
            ("cursor-agent", args, None)
        }
        other => anyhow::bail!("unsupported agent: {other}"),
    };

    if model != "default" {
        args.push("--model".to_string());
        args.push(model.to_string());
    }
    if agent == "gemini" {
        args.push("--prompt".to_string());
    }
    if agent != "claude" {
        args.push(prompt.to_string());
    }

    anyhow::ensure!(!prompt.trim().is_empty(), "task prompt cannot be empty");
    Ok(AgentCommand {
        executable: executable.to_string(),
        args,
        initial_stdin,
    })
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub fn find_executable_in_paths(executable: &str, search_paths: &[PathBuf]) -> Option<PathBuf> {
    let explicit = Path::new(executable);
    if explicit.components().count() > 1 {
        return is_executable_file(explicit).then(|| explicit.to_path_buf());
    }
    search_paths
        .iter()
        .map(|dir| dir.join(executable))
        .find(|candidate| is_executable_file(candidate))
}

pub fn resolve_executable(executable: &str) -> Option<PathBuf> {
    let mut search_paths: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();

    if let Some(home) = dirs::home_dir() {
        for relative in [
            ".local/bin",
            ".npm-global/bin",
            ".volta/bin",
            ".bun/bin",
            ".cargo/bin",
        ] {
            search_paths.push(home.join(relative));
        }
    }
    search_paths.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
    ]);

    find_executable_in_paths(executable, &search_paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_agents_construct_model_aware_structured_commands() {
        let cases = [
            (
                "claude",
                "opus",
                vec![
                    "--print",
                    "--verbose",
                    "--input-format",
                    "stream-json",
                    "--output-format",
                    "stream-json",
                    "--include-partial-messages",
                    "--replay-user-messages",
                    "--model",
                    "opus",
                ],
            ),
            (
                "codex",
                "gpt-5",
                vec!["exec", "--json", "--model", "gpt-5", "Fix the auth flow"],
            ),
            (
                "gemini",
                "gemini-3-pro-preview",
                vec![
                    "--output-format",
                    "stream-json",
                    "--model",
                    "gemini-3-pro-preview",
                    "--prompt",
                    "Fix the auth flow",
                ],
            ),
            (
                "cursor",
                "gpt-5",
                vec![
                    "--print",
                    "--output-format",
                    "stream-json",
                    "--model",
                    "gpt-5",
                    "Fix the auth flow",
                ],
            ),
        ];

        for (agent, model, expected) in cases {
            let command = build_agent_command(agent, model, "Fix the auth flow", None).unwrap();
            assert_eq!(command.args, expected);
        }
    }

    #[test]
    fn default_model_uses_the_agents_own_configured_default() {
        for agent in ["claude", "codex", "gemini", "cursor"] {
            let command = build_agent_command(agent, "default", "Inspect the repo", None).unwrap();
            assert!(
                !command.args.iter().any(|arg| arg == "--model"),
                "{agent} should omit --model when Relay's default sentinel is selected"
            );
            if agent == "claude" {
                assert!(command
                    .initial_stdin
                    .as_deref()
                    .is_some_and(|input| input.contains("Inspect the repo")));
            } else {
                assert_eq!(
                    command.args.last().map(String::as_str),
                    Some("Inspect the repo")
                );
            }
        }
    }

    #[test]
    fn unknown_agent_is_rejected_before_process_launch() {
        let error = build_agent_command("unknown", "default", "Do work", None).unwrap_err();
        assert!(error.to_string().contains("unsupported agent"));
    }

    #[test]
    fn resumed_commands_target_the_provider_session_without_a_picker() {
        let command =
            build_agent_command("codex", "default", "Continue", Some("thread-123")).unwrap();
        assert_eq!(
            command.args,
            vec!["exec", "resume", "thread-123", "--json", "Continue"]
        );
    }

    #[test]
    fn configured_executable_can_be_resolved_from_explicit_search_paths() {
        let temp =
            std::env::temp_dir().join(format!("relay-agent-path-test-{}", std::process::id()));
        std::fs::create_dir_all(&temp).unwrap();
        let executable = temp.join("relay-fake-agent");
        std::fs::write(&executable, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&executable).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&executable, permissions).unwrap();
        }

        let resolved = find_executable_in_paths("relay-fake-agent", &[temp.clone()]);
        assert_eq!(resolved.as_deref(), Some(executable.as_path()));

        let _ = std::fs::remove_file(executable);
        let _ = std::fs::remove_dir(temp);
    }
}
