//! Real detection of an agent CLI on `$PATH` — shells out to the same
//! commands used during manual investigation (`which`, `<cmd> --version`).
//! No capability is inferred here beyond "is it installed and what version
//! does it report."

use std::process::Command;

#[derive(Debug, Clone)]
pub struct Discovery {
    pub detected: bool,
    pub resolved_path: Option<String>,
    pub version: Option<String>,
}

pub fn discover(command: &str) -> Discovery {
    let which = Command::new("which").arg(command).output();
    let resolved_path = match which {
        Ok(out) if out.status.success() => {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if path.is_empty() { None } else { Some(path) }
        }
        _ => None,
    };

    if resolved_path.is_none() {
        return Discovery { detected: false, resolved_path: None, version: None };
    }

    let version = Command::new(command)
        .arg("--version")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|v| !v.is_empty());

    Discovery { detected: true, resolved_path, version }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_command_is_not_detected() {
        let d = discover("single-cli-definitely-does-not-exist-xyz");
        assert!(!d.detected);
        assert!(d.version.is_none());
    }
}
