//! Skill registry (spec section 14): directories under
//! `~/.config/single/skills/<name>/`. Phase 2 scope is local-only —
//! `install` copies a local source directory in; there is no marketplace/
//! network fetch yet (that would need the plugin system from spec section
//! 29, which is later-phase work). A skill is just a directory; SingleCLI
//! doesn't yet interpret its contents (translating it into each agent's
//! native skill mechanism is also later-phase — see docs/architecture.md).

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub fn list(skills_dir: &Path) -> Result<Vec<String>> {
    if !skills_dir.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(skills_dir).with_context(|| format!("reading {}", skills_dir.display()))? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    Ok(names)
}

/// Copies `source` (a directory) into `skills_dir/<name>`. Refuses to
/// overwrite an existing skill of the same name — remove it first.
pub fn install(skills_dir: &Path, name: &str, source: &Path) -> Result<PathBuf> {
    if !source.is_dir() {
        bail!("{} is not a directory", source.display());
    }
    let dest = skills_dir.join(name);
    if dest.exists() {
        bail!("skill '{name}' already exists at {}; remove it first", dest.display());
    }
    std::fs::create_dir_all(skills_dir)?;
    copy_dir_recursive(source, &dest)?;
    Ok(dest)
}

/// Removes `skills_dir/<name>`. Refuses anything that would escape
/// `skills_dir` (defense against a `name` containing `..`).
pub fn remove(skills_dir: &Path, name: &str) -> Result<bool> {
    let dest = skills_dir.join(name);
    let canonical_dir = skills_dir.canonicalize().unwrap_or_else(|_| skills_dir.to_path_buf());
    let canonical_dest = match dest.canonicalize() {
        Ok(p) => p,
        Err(_) => return Ok(false), // doesn't exist
    };
    if !canonical_dest.starts_with(&canonical_dir) {
        bail!("refusing to remove a path outside the skills directory: {}", dest.display());
    }
    std::fs::remove_dir_all(&canonical_dest)?;
    Ok(true)
}

pub fn inspect(skills_dir: &Path, name: &str) -> Result<Option<Vec<String>>> {
    let dir = skills_dir.join(name);
    if !dir.is_dir() {
        return Ok(None);
    }
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        if let Some(n) = entry?.file_name().to_str() {
            entries.push(n.to_string());
        }
    }
    entries.sort();
    Ok(Some(entries))
}

fn copy_dir_recursive(source: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let dest_path = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_then_list_then_inspect() {
        let root = tempfile::tempdir().unwrap();
        let skills_dir = root.path().join("skills");
        let source = root.path().join("source-skill");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "# hi").unwrap();

        install(&skills_dir, "greeting", &source).unwrap();
        assert_eq!(list(&skills_dir).unwrap(), vec!["greeting"]);
        assert_eq!(inspect(&skills_dir, "greeting").unwrap().unwrap(), vec!["SKILL.md"]);
    }

    #[test]
    fn install_refuses_to_overwrite_existing() {
        let root = tempfile::tempdir().unwrap();
        let skills_dir = root.path().join("skills");
        let source = root.path().join("source-skill");
        std::fs::create_dir_all(&source).unwrap();
        install(&skills_dir, "dup", &source).unwrap();
        assert!(install(&skills_dir, "dup", &source).is_err());
    }

    #[test]
    fn remove_deletes_and_reports_missing() {
        let root = tempfile::tempdir().unwrap();
        let skills_dir = root.path().join("skills");
        let source = root.path().join("source-skill");
        std::fs::create_dir_all(&source).unwrap();
        install(&skills_dir, "temp", &source).unwrap();
        assert!(remove(&skills_dir, "temp").unwrap());
        assert!(!remove(&skills_dir, "temp").unwrap());
    }

    #[test]
    fn remove_refuses_path_traversal_when_target_exists_outside_skills_dir() {
        let root = tempfile::tempdir().unwrap();
        let skills_dir = root.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        std::fs::create_dir_all(root.path().join("outside")).unwrap();
        // "../outside" canonicalizes to a real directory outside skills_dir —
        // must error, not silently remove it or report "not found".
        assert!(remove(&skills_dir, "../outside").is_err());
        assert!(root.path().join("outside").exists());
    }

    #[test]
    fn remove_reports_false_for_nonexistent_traversal_target() {
        let root = tempfile::tempdir().unwrap();
        let skills_dir = root.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        // No "outside" directory exists at all -> canonicalize fails -> Ok(false).
        assert!(!remove(&skills_dir, "../nonexistent").unwrap());
    }
}
