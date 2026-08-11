//! The context resolver (spec section 10): repository/git state and
//! project documentation discovery for a working directory.
//!
//! Phase 3 scope is what's listed explicitly in the spec's own diagram for
//! this component minus the parts that depend on subsystems that don't
//! exist yet: repository, git state, branch, changed files, and project
//! documentation are implemented; "relevant source files," "relevant
//! memory," and "previous agent actions" require an actual relevance-
//! ranking/selection step over a real task, which needs the Phase 4
//! orchestrator to have a task to resolve context *for*. This returns a
//! snapshot of ambient project state, not yet a task-scoped selection.

use single_protocol::ProjectContext;
use std::path::{Path, PathBuf};
use std::process::Command;

const DOC_CANDIDATES: &[&str] =
    &["README.md", "CLAUDE.md", "AGENTS.md", "CONTRIBUTING.md", ".single/config.yaml"];

pub fn resolve(cwd: &Path) -> ProjectContext {
    let repo_root = git_output(cwd, &["rev-parse", "--show-toplevel"]);
    let branch = git_output(cwd, &["rev-parse", "--abbrev-ref", "HEAD"]);
    let changed_files = git_status_files(cwd);

    let doc_search_root: PathBuf = repo_root.as_ref().map(PathBuf::from).unwrap_or_else(|| cwd.to_path_buf());
    let project_docs = DOC_CANDIDATES
        .iter()
        .filter(|name| doc_search_root.join(name).is_file())
        .map(|name| name.to_string())
        .collect();

    ProjectContext { cwd: cwd.display().to_string(), repo_root, branch, changed_files, project_docs }
}

fn git_output(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git").current_dir(cwd).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

fn git_status_files(cwd: &Path) -> Vec<String> {
    let Some(output) = Command::new("git").current_dir(cwd).args(["status", "--porcelain"]).output().ok() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        // porcelain format: "XY path" — skip the 2-char status + space.
        .filter_map(|line| line.get(3..).map(|s| s.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_git_directory_resolves_with_no_repo_state() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = resolve(dir.path());
        assert!(ctx.repo_root.is_none());
        assert!(ctx.branch.is_none());
        assert!(ctx.changed_files.is_empty());
    }

    #[test]
    fn finds_project_docs_by_known_filenames() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("README.md"), "# hi").unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "instructions").unwrap();
        let ctx = resolve(dir.path());
        assert!(ctx.project_docs.contains(&"README.md".to_string()));
        assert!(ctx.project_docs.contains(&"CLAUDE.md".to_string()));
        assert!(!ctx.project_docs.contains(&"AGENTS.md".to_string()));
    }

    #[test]
    fn resolves_real_git_state_in_this_repo() {
        // This crate's own repo root — a real, non-mocked git invocation,
        // matching the "verify against real behavior" approach used
        // throughout this project rather than trusting an assumption.
        let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let ctx = resolve(here);
        assert!(ctx.repo_root.is_some(), "expected this crate to resolve inside a real git repo");
        assert!(ctx.branch.is_some());
    }
}
