//! Attaches a board card's prompt to a live `claude` CLI session running in Terminal.app for
//! that card's project, or opens a new Terminal window for one — see
//! `commands::launch_or_attach_session`. macOS-only: the underlying mechanism is Terminal.app
//! + System Events AppleScript automation, which has no equivalent on other platforms.

/// Runs `attach_session.applescript` (see that file's doc comment for the full behavior):
/// finds an existing Terminal tab already running `claude` in `project_path` and pastes
/// `prompt` into it, or opens a new Terminal window there (resuming `resume_session_id` if
/// given) and pastes `prompt` once it's booted. Returns a short outcome string for logging —
/// `"attached_existing_tab"`, `"resumed_in_new_window"`, or `"started_new_window"`.
///
/// The prompt travels via the system clipboard (Cmd+V in the script) rather than simulated
/// keystrokes of the literal text, since Claude Code's interactive input submits on a bare
/// Return — typing a multi-line prompt character-by-character would submit it early at the
/// first newline. The caller's previous clipboard contents are restored on the way out,
/// best-effort (a restore failure is logged, never turned into an error for an otherwise
/// successful attach/launch).
///
/// Requires the app to have macOS Accessibility (System Events keystrokes) and Automation
/// (controlling Terminal.app) permission — on first use macOS will prompt for these; if
/// denied, this returns an `Err` describing the `osascript` failure rather than silently
/// doing nothing.
#[cfg(target_os = "macos")]
pub fn attach_or_launch(
    project_path: &str,
    resume_session_id: Option<&str>,
    prompt: &str,
) -> anyhow::Result<String> {
    let previous_clipboard = read_clipboard();
    set_clipboard(prompt)?;

    let result = run_applescript(project_path, resume_session_id.unwrap_or(""));

    if let Some(previous) = previous_clipboard {
        if let Err(e) = set_clipboard(&previous) {
            log::warn!("failed to restore clipboard after pasting session prompt: {e:#}");
        }
    }

    result
}

#[cfg(not(target_os = "macos"))]
pub fn attach_or_launch(
    _project_path: &str,
    _resume_session_id: Option<&str>,
    _prompt: &str,
) -> anyhow::Result<String> {
    anyhow::bail!("attaching/launching a Claude Code terminal session is only supported on macOS")
}

#[cfg(target_os = "macos")]
const ATTACH_SESSION_SCRIPT: &str = include_str!("../resources/attach_session.applescript");

#[cfg(target_os = "macos")]
fn run_applescript(project_path: &str, resume_id: &str) -> anyhow::Result<String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new("osascript")
        .arg("-")
        .arg(project_path)
        .arg(resume_id)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Dropped (closing the pipe) at the end of this statement, which is what tells
    // osascript it's seen the whole script and can start running it.
    child
        .stdin
        .take()
        .expect("stdin was requested via Stdio::piped()")
        .write_all(ATTACH_SESSION_SCRIPT.as_bytes())?;

    let output = child.wait_with_output()?;
    if !output.status.success() {
        anyhow::bail!(
            "osascript failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(target_os = "macos")]
fn read_clipboard() -> Option<String> {
    let output = std::process::Command::new("pbpaste").output().ok()?;
    if output.status.success() {
        String::from_utf8(output.stdout).ok()
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn set_clipboard(text: &str) -> anyhow::Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new("pbcopy").stdin(Stdio::piped()).spawn()?;
    child
        .stdin
        .take()
        .expect("stdin was requested via Stdio::piped()")
        .write_all(text.as_bytes())?;
    child.wait()?;
    Ok(())
}
