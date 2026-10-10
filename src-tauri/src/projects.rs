//! Which folders Relay lists as projects, and adding new ones.
//!
//! Every cwd an agent runs in gets a `projects` row at ingest, but only folders confirmed to
//! be GitHub repositories the user's `gh` can access are listed (`queries::list_projects`).
//! Confirmation happens here, on a background thread (`spawn_github_verifier`), and
//! synchronously when the user clones or adds a project.
//!
//! Like the idle sweep in `lib.rs`, the verifier runs **gather** (DB lock held, one cheap
//! query), **compute** (lock released: git and `gh` processes, which can take seconds or hang
//! on the network), **write** (lock re-acquired briefly, per project). The shared
//! `Mutex<Connection>` is never held across a `gh` call.

use crate::db::{queries, Db};
use crate::parser;
use crate::ship::git::{self, GithubCheck};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// A verified project is re-checked this long after its last check (losing access is rare).
const VERIFIED_RECHECK_SECS: i64 = 24 * 60 * 60;
/// An unverified one sooner: the user may have just added a remote or run `gh auth login`.
const UNVERIFIED_RECHECK_SECS: i64 = 15 * 60;
/// How often the verifier looks for stale rows when nothing wakes it.
const VERIFIER_TICK: Duration = Duration::from_secs(5 * 60);
/// After a wake, wait this long so a burst of ingest wakes becomes one pass.
const WAKE_DEBOUNCE: Duration = Duration::from_secs(2);
/// Folder name under the home directory that `clone_project` clones into by default. Not
/// `~/Relay`: on macOS's case-insensitive disk that is the same folder as a `~/relay`
/// checkout, so clones would land inside another repository.
const DEFAULT_CLONE_DIR: &str = "RelayProjects";

/// GitHub's own limits: usernames/orgs are 1–39 alphanumerics or single hyphens, not starting
/// with one; repository names are 1–100 of alphanumerics, `-`, `_`, `.`, and not `.`/`..`.
fn valid_owner(owner: &str) -> bool {
    (1..=39).contains(&owner.len())
        && !owner.starts_with('-')
        && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn valid_name(name: &str) -> bool {
    (1..=100).contains(&name.len())
        && name != "."
        && name != ".."
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Exactly `owner/name` (an optional `.git` suffix allowed), validated.
pub fn parse_bare_repo(input: &str) -> Option<(String, String)> {
    let (owner, name) = input.split_once('/')?;
    let name = name.strip_suffix(".git").unwrap_or(name);
    (valid_owner(owner) && valid_name(name)).then(|| (owner.to_string(), name.to_string()))
}

/// Parses a pasted GitHub link into `(owner, name)`. Accepts `https://github.com/o/n` (with
/// `.git`, a trailing slash, or deeper paths such as `/tree/main`, a query or fragment),
/// `ssh://git@github.com/o/n.git`, `git@github.com:o/n.git`, `github.com/o/n`, and bare `o/n`.
/// Rejects everything else — other hosts, `ext::`/`file:` and other schemes, leading `-`
/// (option injection into gh/git), and whitespace or control characters inside the input —
/// so only a normalized `owner/name` ever reaches `gh`.
pub fn parse_github_repo(input: &str) -> Option<(String, String)> {
    let input = input.trim();
    if input.is_empty()
        || input.starts_with('-')
        || input.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return None;
    }
    let path = if let Some(rest) = input.strip_prefix("git@github.com:") {
        rest
    } else if let Some((scheme, rest)) = input.split_once("://") {
        let scheme = scheme.to_ascii_lowercase();
        if !matches!(scheme.as_str(), "https" | "http" | "ssh" | "git") {
            return None;
        }
        let (authority, path) = rest.split_once('/')?;
        let host = authority.rsplit('@').next()?.to_ascii_lowercase();
        if host != "github.com" && host != "www.github.com" {
            return None;
        }
        path
    } else if let Some(rest) = input
        .strip_prefix("github.com/")
        .or_else(|| input.strip_prefix("www.github.com/"))
    {
        rest
    } else {
        // Bare `owner/name`: nothing else may be in it (no `:` scheme tricks, no extra path).
        return parse_bare_repo(input);
    };
    let path = path.split(['?', '#']).next()?;
    let mut segments = path.split('/');
    let owner = segments.next()?;
    let name = segments.next()?;
    parse_bare_repo(&format!("{owner}/{name}"))
}

/// What `clone_project` should do at the destination folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloneTarget {
    /// Nothing there (or an empty folder): clone into it.
    Clone(PathBuf),
    /// Already a checkout of this same repository: just register it.
    AlreadyCloned(PathBuf),
}

/// Decides where `owner/name` goes under `parent`: `<parent>/<name>`. Never overwrites — an
/// existing non-empty folder is only accepted when its own GitHub remotes include this
/// repository (checked locally, no network); anything else is a clear error.
pub fn plan_clone_target(parent: &Path, owner: &str, name: &str) -> anyhow::Result<CloneTarget> {
    let dest = parent.join(name);
    if !dest.exists() {
        return Ok(CloneTarget::Clone(dest));
    }
    anyhow::ensure!(
        dest.is_dir(),
        "{} already exists and is not a folder",
        dest.display()
    );
    if std::fs::read_dir(&dest)?.next().is_none() {
        return Ok(CloneTarget::Clone(dest));
    }
    let wanted = format!("{owner}/{name}");
    let same_repo = git::local_github_remotes(&dest)
        .unwrap_or_default()
        .iter()
        .any(|repo| repo.eq_ignore_ascii_case(&wanted));
    anyhow::ensure!(
        same_repo,
        "{} already exists and is not a checkout of {wanted}; move it or pick another folder",
        dest.display()
    );
    Ok(CloneTarget::AlreadyCloned(dest))
}

/// `~/RelayProjects` (see `DEFAULT_CLONE_DIR`).
pub fn default_clone_parent() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(DEFAULT_CLONE_DIR))
}

/// A project the user just cloned or added, as returned to the frontend.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AddedProject {
    pub id: String,
    pub name: String,
    pub path: String,
    pub github_repo: String,
}

/// Inserts (or refreshes) a project row for a verified folder and makes sure it has a board,
/// so the Workspace can open it before any session ran there. Uses the same id scheme as
/// ingest, so later sessions in this folder land on this same project.
pub fn register_project(
    conn: &rusqlite::Connection,
    path: &Path,
    github_repo: &str,
    now: i64,
) -> anyhow::Result<AddedProject> {
    let path = path.to_string_lossy().trim_end_matches('/').to_string();
    let id = parser::project_id_for_path(&path);
    let name = Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&path)
        .to_string();
    queries::upsert_project(conn, &id, &name, &path, now)?;
    queries::set_project_github(conn, &id, Some(github_repo), now)?;
    queries::ensure_board_for_project(conn, &id)?;
    Ok(AddedProject {
        id,
        name,
        path,
        github_repo: github_repo.to_string(),
    })
}

/// Managed state: wakes the verifier. A bounded channel of one, so any number of wakes while
/// a pass runs coalesce into a single follow-up pass.
pub struct GithubVerifier(SyncSender<()>);

/// Asks the verifier to look for unchecked projects soon (e.g. ingest just saw a new cwd).
/// Cheap and non-blocking; a no-op before the verifier is running.
pub fn wake_github_verifier(app: &AppHandle) {
    if let Some(verifier) = app.try_state::<GithubVerifier>() {
        let _ = verifier.0.try_send(());
    }
}

/// Starts the verifier thread: one pass at startup (every project is unchecked after the
/// migration), then whenever woken or every `VERIFIER_TICK`. A dedicated OS thread rather than
/// an async task because every check is blocking process I/O.
pub fn spawn_github_verifier(app: AppHandle) {
    let (wake, woken) = mpsc::sync_channel::<()>(1);
    app.manage(GithubVerifier(wake));
    std::thread::spawn(move || loop {
        verify_pass(&app);
        match woken.recv_timeout(VERIFIER_TICK) {
            Ok(()) => {
                std::thread::sleep(WAKE_DEBOUNCE);
                while woken.try_recv().is_ok() {}
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    });
}

fn verify_pass(app: &AppHandle) {
    let db = app.state::<Db>();
    // Gather (locked, one query).
    let due = {
        let Ok(conn) = db.0.lock() else { return };
        match queries::projects_needing_github_check(
            &conn,
            chrono::Utc::now().timestamp(),
            VERIFIED_RECHECK_SECS,
            UNVERIFIED_RECHECK_SECS,
        ) {
            Ok(due) => due,
            Err(error) => {
                log::warn!("github verifier: failed to list projects to check: {error:#}");
                return;
            }
        }
    };
    for project in due {
        // Compute (unlocked): git + gh processes.
        let check = git::check_folder_github(Path::new(&project.path));
        // Write (locked briefly), one project at a time so each shows up as soon as it's known.
        let now = chrono::Utc::now().timestamp();
        let changed = {
            let Ok(conn) = db.0.lock() else { return };
            match &check {
                GithubCheck::Verified(repo) => {
                    queries::set_project_github(&conn, &project.id, Some(repo), now)
                }
                GithubCheck::NotGithub(_) => {
                    queries::set_project_github(&conn, &project.id, None, now)
                }
                GithubCheck::Inconclusive(_) => {
                    queries::touch_project_github_check(&conn, &project.id, now).map(|()| false)
                }
            }
        };
        match (&check, &changed) {
            (_, Err(error)) => log::warn!(
                "github verifier: failed to record check for {}: {error:#}",
                project.path
            ),
            (GithubCheck::Inconclusive(reason), _) => log::warn!(
                "github verifier: couldn't check {} ({reason}); keeping its previous status",
                project.path
            ),
            (GithubCheck::NotGithub(reason), Ok(true)) => log::warn!(
                "github verifier: no longer listing {} ({reason})",
                project.path
            ),
            _ => {}
        }
        if matches!(changed, Ok(true)) {
            let _ = app.emit(
                "data-changed",
                serde_json::json!({ "entity": "project", "kind": "updated" }),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(input: &str) -> Option<String> {
        parse_github_repo(input).map(|(o, n)| format!("{o}/{n}"))
    }

    #[test]
    fn accepts_the_usual_ways_to_paste_a_github_repo() {
        for input in [
            "https://github.com/TanmayDabhade/Relay",
            "https://github.com/TanmayDabhade/Relay.git",
            "https://github.com/TanmayDabhade/Relay/",
            "https://github.com/TanmayDabhade/Relay/tree/main/src",
            "https://github.com/TanmayDabhade/Relay?tab=readme#top",
            "http://www.github.com/TanmayDabhade/Relay",
            "HTTPS://GitHub.com/TanmayDabhade/Relay",
            "ssh://git@github.com/TanmayDabhade/Relay.git",
            "git@github.com:TanmayDabhade/Relay.git",
            "github.com/TanmayDabhade/Relay",
            "TanmayDabhade/Relay",
            "  TanmayDabhade/Relay\n",
        ] {
            assert_eq!(
                pair(input).as_deref(),
                Some("TanmayDabhade/Relay"),
                "{input}"
            );
        }
        assert_eq!(
            pair("o/my.repo_name-2").as_deref(),
            Some("o/my.repo_name-2")
        );
    }

    #[test]
    fn rejects_other_hosts_schemes_and_injection_attempts() {
        for input in [
            "",
            "Relay",
            "o/n/extra",
            "https://gitlab.com/o/n",
            "https://github.com.evil.com/o/n",
            "https://evil.com/github.com/o/n",
            "git@gitlab.com:o/n.git",
            "ext::sh -c touch% /tmp/pwned",
            "ext::sh",
            "file:///etc/passwd",
            "file://github.com/o/n",
            "--upload-pack=touch /tmp/x",
            "-o/n",
            "o/-n",
            "o/..",
            "o/.",
            "o n/x",
            "o/n x",
            "o/n\u{0}",
            "https://github.com/o",
            "https://github.com//n",
            "https://github.com/o/n\tx",
            "o_x/n",
        ] {
            assert_eq!(pair(input), None, "{input:?}");
        }
        assert_eq!(pair(&format!("{}/n", "a".repeat(40))), None);
    }

    #[test]
    fn clone_targets_never_overwrite_an_unrelated_folder() {
        let base = std::env::temp_dir().join(format!(
            "relay-clone-target-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&base).unwrap();

        // Nothing there yet, or an empty folder: clone into it.
        assert_eq!(
            plan_clone_target(&base, "o", "fresh").unwrap(),
            CloneTarget::Clone(base.join("fresh"))
        );
        std::fs::create_dir_all(base.join("empty")).unwrap();
        assert_eq!(
            plan_clone_target(&base, "o", "empty").unwrap(),
            CloneTarget::Clone(base.join("empty"))
        );

        // A name collision with unrelated content, or a file: refused.
        std::fs::create_dir_all(base.join("taken")).unwrap();
        std::fs::write(base.join("taken/notes.txt"), "mine").unwrap();
        let error = plan_clone_target(&base, "o", "taken").unwrap_err();
        assert!(
            error.to_string().contains("not a checkout of o/taken"),
            "{error}"
        );
        std::fs::write(base.join("file"), "x").unwrap();
        assert!(plan_clone_target(&base, "o", "file").is_err());

        // An existing checkout of the same repo (any remote, any case) is just registered;
        // a checkout of another repo with the same name is refused.
        let run = |dir: &Path, args: &[&str]| {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .status()
                .unwrap();
            assert!(status.success());
        };
        let same = base.join("same");
        std::fs::create_dir_all(&same).unwrap();
        run(&same, &["init", "-q"]);
        run(
            &same,
            &["remote", "add", "origin", "git@github.com:O/Same.git"],
        );
        assert_eq!(
            plan_clone_target(&base, "o", "same").unwrap(),
            CloneTarget::AlreadyCloned(same.clone())
        );
        assert!(plan_clone_target(&base, "someone-else", "same").is_err());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn registering_uses_the_ingest_id_and_creates_a_board() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        for sql in [
            include_str!("../migrations/0001_init.sql"),
            include_str!("../migrations/0003_kanban.sql"),
            include_str!("../migrations/0011_session_usage.sql"),
            include_str!("../migrations/0013_project_github.sql"),
        ] {
            conn.execute_batch(sql).unwrap();
        }
        let added = register_project(
            &conn,
            Path::new("/Users/me/RelayProjects/app/"),
            "me/app",
            10,
        )
        .unwrap();
        assert_eq!(added.path, "/Users/me/RelayProjects/app");
        assert_eq!(added.name, "app");
        assert_eq!(
            added.id,
            parser::project_id_for_path("/Users/me/RelayProjects/app")
        );
        let listed = queries::list_projects(&conn).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].github_repo.as_deref(), Some("me/app"));
        let boards: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM boards WHERE project_id = ?1",
                [&added.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(boards, 1);
        // Registering again is idempotent.
        register_project(
            &conn,
            Path::new("/Users/me/RelayProjects/app"),
            "me/app",
            20,
        )
        .unwrap();
        assert_eq!(queries::list_projects(&conn).unwrap().len(), 1);
    }
}
