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
}
