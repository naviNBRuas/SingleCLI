//! Git worktree management (spec section 21): give each task its own
//! isolated working tree and branch, off a real repository, via real `git
//! worktree` subprocess calls — not a fabricated isolation mechanism.
//!
//! Phase 4 scope: single-agent, single-worktree-per-task isolation. Cross-
//! worktree coordination (locks, merge/conflict resolution across several
//! concurrent agents) is future work once there's more than one agent
//! actually running concurrently against the same repo — see
//! `docs/architecture.md`.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Creates a worktree at `worktree_path` off `repo_root`, on a new branch
/// `branch_name` based on the repo's current HEAD.
pub fn add(repo_root: &Path, worktree_path: &Path, branch_name: &str) -> Result<()> {
    if !is_git_repo(repo_root) {
        bail!("{} is not a git repository", repo_root.display());
    }
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["worktree", "add", "-b", branch_name])
        .arg(worktree_path)
        .output()
        .context("spawning git worktree add")?;
    if !output.status.success() {
        bail!("git worktree add failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
}

/// Removes a worktree. `force` matches `git worktree remove --force`
/// (needed if the worktree has uncommitted changes) — callers decide
/// whether to force based on whether they intend to discard task output,
/// not this function.
pub fn remove(repo_root: &Path, worktree_path: &Path, force: bool) -> Result<()> {
    let mut args = vec!["worktree".to_string(), "remove".to_string()];
    if force {
        args.push("--force".to_string());
    }
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(&args)
        .arg(worktree_path)
        .output()
        .context("spawning git worktree remove")?;
    if !output.status.success() {
        bail!("git worktree remove failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
}

pub fn list(repo_root: &Path) -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .context("spawning git worktree list")?;
    if !output.status.success() {
        bail!("git worktree list failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)
        .collect())
}

/// Diffs `branch` against the repo's current `HEAD` (`git diff
/// HEAD...branch`) — the same three-dot range a GitHub PR view shows,
/// i.e. what actually landed on `branch` since it forked off, not
/// polluted by anything that's happened on the base branch since. Never
/// merges anything — see `merge` below, kept as a deliberately separate
/// call so a caller looks before deciding.
pub fn diff(repo_root: &Path, branch: &str) -> Result<String> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["diff", &format!("HEAD...{branch}")])
        .output()
        .context("spawning git diff")?;
    if !output.status.success() {
        bail!("git diff failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Merges `branch` into the repo's current `HEAD` (`git merge --no-ff
/// branch`) — `--no-ff` always creates a merge commit, so the fact that
/// this went through an isolated worktree stays visible in history
/// rather than silently fast-forwarding. Per `docs/architecture.md`:
/// "branches are never auto-merged; that stays a human decision" — this
/// function performs the merge once a caller has explicitly decided to
/// (see `diff` above, meant to be called first), it does not decide FOR
/// the caller.
pub fn merge(repo_root: &Path, branch: &str) -> Result<String> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["merge", "--no-ff", branch, "-m", &format!("Merge branch '{branch}'")])
        .output()
        .context("spawning git merge")?;
    if !output.status.success() {
        let merge_stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        // A real conflict leaves conflict markers in the working tree and
        // `.git/MERGE_HEAD` in place — a genuine mid-merge repo state. This
        // is reachable unattended (an MCP agent calling
        // `worktree_merge_apply`), so leaving that half-done state around
        // for a human to notice later is worse than restoring the
        // pre-merge state ourselves and surfacing a clear error, matching
        // this project's general preference for known-clean over half-done.
        let abort_output = Command::new("git")
            .current_dir(repo_root)
            .args(["merge", "--abort"])
            .output()
            .context("spawning git merge --abort")?;
        if !abort_output.status.success() {
            bail!(
                "git merge failed ({merge_stderr}) AND git merge --abort also failed ({}); repo may be left in a conflicted state, manual cleanup required",
                String::from_utf8_lossy(&abort_output.stderr)
            );
        }
        bail!("git merge failed and was aborted (repo restored to its pre-merge state): {merge_stderr}");
    }
    Ok(format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

fn is_git_repo(path: &Path) -> bool {
    Command::new("git")
        .current_dir(path)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo(dir: &Path) {
        let run = |args: &[&str]| {
            let status = Command::new("git").current_dir(dir).args(args).status().unwrap();
            assert!(status.success(), "git {:?} failed", args);
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "Test"]);
        std::fs::write(dir.join("README.md"), "hi").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "initial"]);
    }

    #[test]
    fn add_creates_a_real_worktree_on_a_new_branch() {
        let repo = tempfile::tempdir().unwrap();
        init_repo(repo.path());
        let worktree_parent = tempfile::tempdir().unwrap();
        let worktree_path = worktree_parent.path().join("task-1");

        add(repo.path(), &worktree_path, "single/task-1").unwrap();
        assert!(worktree_path.join("README.md").is_file());

        let worktrees = list(repo.path()).unwrap();
        assert!(worktrees.iter().any(|p| p == &worktree_path.canonicalize().unwrap() || p == &worktree_path));
    }

    #[test]
    fn remove_deletes_the_worktree() {
        let repo = tempfile::tempdir().unwrap();
        init_repo(repo.path());
        let worktree_parent = tempfile::tempdir().unwrap();
        let worktree_path = worktree_parent.path().join("task-2");

        add(repo.path(), &worktree_path, "single/task-2").unwrap();
        remove(repo.path(), &worktree_path, false).unwrap();
        assert!(!worktree_path.exists());
    }

    #[test]
    fn add_fails_outside_a_git_repo() {
        let not_a_repo = tempfile::tempdir().unwrap();
        let worktree_path = tempfile::tempdir().unwrap().path().join("task-3");
        assert!(add(not_a_repo.path(), &worktree_path, "single/task-3").is_err());
    }

    #[test]
    fn diff_shows_changes_made_on_the_worktree_branch() {
        let repo = tempfile::tempdir().unwrap();
        init_repo(repo.path());
        let worktree_parent = tempfile::tempdir().unwrap();
        let worktree_path = worktree_parent.path().join("task-diff");

        add(repo.path(), &worktree_path, "single/task-diff").unwrap();
        std::fs::write(worktree_path.join("new-file.txt"), "hello from the worktree").unwrap();
        let status = Command::new("git").current_dir(&worktree_path).args(["add", "."]).status().unwrap();
        assert!(status.success());
        let status = Command::new("git").current_dir(&worktree_path).args(["commit", "-q", "-m", "add new-file"]).status().unwrap();
        assert!(status.success());

        let diff_output = diff(repo.path(), "single/task-diff").unwrap();
        assert!(diff_output.contains("new-file.txt"));
        assert!(diff_output.contains("hello from the worktree"));
    }

    #[test]
    fn merge_brings_worktree_changes_into_the_main_branch() {
        let repo = tempfile::tempdir().unwrap();
        init_repo(repo.path());
        let worktree_parent = tempfile::tempdir().unwrap();
        let worktree_path = worktree_parent.path().join("task-merge");

        add(repo.path(), &worktree_path, "single/task-merge").unwrap();
        std::fs::write(worktree_path.join("merged-file.txt"), "merge me").unwrap();
        let status = Command::new("git").current_dir(&worktree_path).args(["add", "."]).status().unwrap();
        assert!(status.success());
        let status = Command::new("git").current_dir(&worktree_path).args(["commit", "-q", "-m", "add merged-file"]).status().unwrap();
        assert!(status.success());

        merge(repo.path(), "single/task-merge").unwrap();
        assert!(repo.path().join("merged-file.txt").is_file());
    }

    #[test]
    fn merge_aborts_and_restores_a_clean_state_on_a_real_conflict() {
        let repo = tempfile::tempdir().unwrap();
        init_repo(repo.path());
        let worktree_parent = tempfile::tempdir().unwrap();
        let worktree_path = worktree_parent.path().join("task-conflict");

        add(repo.path(), &worktree_path, "single/task-conflict").unwrap();

        // Conflicting edit on the worktree branch.
        std::fs::write(worktree_path.join("README.md"), "changed on the worktree branch").unwrap();
        let status = Command::new("git").current_dir(&worktree_path).args(["add", "."]).status().unwrap();
        assert!(status.success());
        let status = Command::new("git")
            .current_dir(&worktree_path)
            .args(["commit", "-q", "-m", "conflicting change on worktree branch"])
            .status()
            .unwrap();
        assert!(status.success());

        // Conflicting edit on the main branch (repo_root), same lines.
        std::fs::write(repo.path().join("README.md"), "changed on the main branch").unwrap();
        let status = Command::new("git").current_dir(repo.path()).args(["add", "."]).status().unwrap();
        assert!(status.success());
        let status = Command::new("git")
            .current_dir(repo.path())
            .args(["commit", "-q", "-m", "conflicting change on main branch"])
            .status()
            .unwrap();
        assert!(status.success());

        let result = merge(repo.path(), "single/task-conflict");
        assert!(result.is_err(), "expected a real conflict to produce an Err");
        let message = format!("{:#}", result.unwrap_err());
        assert!(
            message.contains("aborted"),
            "expected the error to mention the merge was aborted, got: {message}"
        );

        // The repo must be back in a clean, non-mid-merge state.
        assert!(
            !repo.path().join(".git").join("MERGE_HEAD").exists(),
            "MERGE_HEAD should not exist after an aborted merge"
        );
        let status_output = Command::new("git")
            .current_dir(repo.path())
            .args(["status", "--porcelain"])
            .output()
            .unwrap();
        assert!(
            String::from_utf8_lossy(&status_output.stdout).trim().is_empty(),
            "expected a clean working tree after the aborted merge"
        );
    }

    #[test]
    fn diff_fails_for_a_nonexistent_branch() {
        let repo = tempfile::tempdir().unwrap();
        init_repo(repo.path());
        assert!(diff(repo.path(), "no-such-branch").is_err());
    }
}
