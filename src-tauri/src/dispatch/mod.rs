use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub mod looping;
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
    find_executable_in_paths(executable, agent_search_paths())
}

const LOGIN_PATH_MARKER: &str = "__RELAY_LOGIN_PATH__";
// Generous: this runs once, in the background at startup, and a real zsh with plugin
// managers was measured at ~4s cold.
const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(10);

/// The directory list used both to resolve an agent executable and as the `PATH` a spawned
/// agent inherits. A Finder-launched app gets launchd's bare `/usr/bin:/bin:/usr/sbin:/sbin`,
/// so without this a provider CLI starts but everything *it* shells out to — `node` for
/// plugin hooks, `git`, `npx` MCP servers — is "command not found" (observed: every Claude
/// `SessionEnd` hook failing under Relay). Computed once: the login-shell probe costs a
/// shell startup, and `OnceLock` makes concurrent first callers wait rather than re-probe.
pub fn agent_search_paths() -> &'static [PathBuf] {
    static PATHS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    PATHS.get_or_init(|| {
        let login = login_shell_path();
        if login.is_none() {
            log::warn!("could not read login shell PATH; agents get a fallback PATH");
        }
        let inherited = std::env::var_os("PATH");
        merge_search_paths(login.as_deref(), inherited.as_deref(), dirs::home_dir())
    })
}

/// `agent_search_paths` joined into a `PATH` value for a spawned agent process.
pub fn agent_path_env() -> OsString {
    std::env::join_paths(agent_search_paths()).unwrap_or_else(|error| {
        log::warn!("agent PATH contained an unjoinable entry: {error}");
        std::env::var_os("PATH").unwrap_or_default()
    })
}

/// Asks the user's login shell for its `PATH`. Interactive (`-i`) because zsh users commonly
/// set PATH in `.zshrc`, which a non-interactive login shell skips. rc files can print
/// banners, so the value is fenced by markers; a shell that hangs (a prompt, a slow plugin
/// manager) is killed after `LOGIN_SHELL_TIMEOUT` and we fall back to the static list.
fn login_shell_path() -> Option<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let mut child = Command::new(shell)
        .args([
            "-ilc",
            &format!("printf '{LOGIN_PATH_MARKER}%s{LOGIN_PATH_MARKER}' \"$PATH\""),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + LOGIN_SHELL_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut output = String::new();
    child.stdout.take()?.read_to_string(&mut output).ok()?;
    extract_marked_path(&output)
}

fn extract_marked_path(output: &str) -> Option<String> {
    let start = output.find(LOGIN_PATH_MARKER)? + LOGIN_PATH_MARKER.len();
    let len = output[start..].find(LOGIN_PATH_MARKER)?;
    let path = output[start..start + len].trim();
    (!path.is_empty()).then(|| path.to_string())
}

/// Login-shell entries first (the user's own ordering wins), then the inherited PATH, then
/// well-known install locations, deduplicated preserving first occurrence.
fn merge_search_paths(
    login: Option<&str>,
    inherited: Option<&std::ffi::OsStr>,
    home: Option<PathBuf>,
) -> Vec<PathBuf> {
    let mut search_paths: Vec<PathBuf> = Vec::new();
    if let Some(login) = login {
        search_paths.extend(std::env::split_paths(login));
    }
    if let Some(inherited) = inherited {
        search_paths.extend(std::env::split_paths(inherited));
    }

    if let Some(home) = home {
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
        PathBuf::from("/bin"),
        PathBuf::from("/usr/sbin"),
        PathBuf::from("/sbin"),
    ]);

    let mut seen = std::collections::HashSet::new();
    search_paths.retain(|dir| !dir.as_os_str().is_empty() && seen.insert(dir.clone()));
    search_paths
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
    fn login_shell_path_is_extracted_from_between_rc_file_noise() {
        let output = format!(
            "Welcome back!\n{LOGIN_PATH_MARKER}/usr/local/opt/node@24/bin:/usr/bin{LOGIN_PATH_MARKER}\n"
        );
        assert_eq!(
            extract_marked_path(&output).as_deref(),
            Some("/usr/local/opt/node@24/bin:/usr/bin")
        );
        assert_eq!(extract_marked_path("no markers here"), None);
        assert_eq!(
            extract_marked_path(&format!("{LOGIN_PATH_MARKER}{LOGIN_PATH_MARKER}")),
            None
        );
    }

    #[test]
    fn search_paths_put_login_shell_first_and_keep_launchd_fallbacks() {
        let merged = merge_search_paths(
            Some("/usr/local/opt/node@24/bin:/usr/bin"),
            Some(std::ffi::OsStr::new("/usr/bin:/bin:/usr/sbin:/sbin")),
            Some(PathBuf::from("/Users/me")),
        );
        assert_eq!(merged[0], PathBuf::from("/usr/local/opt/node@24/bin"));
        assert_eq!(merged[1], PathBuf::from("/usr/bin"));
        assert!(merged.contains(&PathBuf::from("/Users/me/.local/bin")));
        assert!(merged.contains(&PathBuf::from("/opt/homebrew/bin")));
        let unique: std::collections::HashSet<_> = merged.iter().collect();
        assert_eq!(unique.len(), merged.len(), "entries must be deduplicated");
    }

    #[test]
    fn search_paths_without_a_login_shell_still_cover_common_installs() {
        let merged = merge_search_paths(None, None, None);
        for dir in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
            assert!(merged.contains(&PathBuf::from(dir)), "missing {dir}");
        }
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
