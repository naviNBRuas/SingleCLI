//! Reads/writes `~/.config/opencode/opencode.jsonc`'s `mcp` key. Format
//! confirmed by direct inspection of a real file: `"type": "local"`,
//! `"command": [...]` (array, not a single string), `"environment": {...}`,
//! `"enabled": bool`.
//!
//! JSONC allows `//` and `/* */` comments that plain `serde_json` can't
//! parse. Phase 1 handles this with a minimal comment stripper below rather
//! than pulling in a JSONC crate — it is deliberately conservative (only
//! strips comments outside string literals) but, like the TOML writer,
//! re-serializing loses comments and reformats the file. This is an adapter
//! *emulation*, not a native capability — documented here and in
//! docs/architecture.md rather than silently discarding user comments.

use single_protocol::McpServerSpec;
use anyhow::{Context, Result};
use serde_json::{Map, Value};
use std::path::Path;

/// Strips `//line` and `/* block */` comments that are not inside a JSON
/// string literal, so the rest can be parsed with `serde_json`.
fn strip_jsonc_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }

        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev = ' ';
                for c in chars.by_ref() {
                    if prev == '*' && c == '/' {
                        break;
                    }
                    prev = c;
                }
            }
            _ => out.push(c),
        }
    }
    out
}

pub fn apply(path: &Path, servers: &[McpServerSpec]) -> Result<Value> {
    let mut root: Value = if path.exists() {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let stripped = strip_jsonc_comments(&text);
        serde_json::from_str(&stripped)
            .with_context(|| format!("parsing {} as JSONC", path.display()))?
    } else {
        Value::Object(Map::new())
    };

    let root_obj = root.as_object_mut().context("opencode.jsonc root is not a JSON object")?;
    let mcp_obj = root_obj
        .entry("mcp")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("mcp is not a JSON object")?;

    for server in servers {
        let mut command_parts = vec![server.command.clone()];
        command_parts.extend(server.args.iter().cloned());

        let mut entry = Map::new();
        entry.insert("type".into(), Value::String("local".into()));
        entry.insert("command".into(), Value::Array(command_parts.into_iter().map(Value::String).collect()));
        if !server.env.is_empty() {
            let env: Map<String, Value> =
                server.env.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect();
            entry.insert("environment".into(), Value::Object(env));
        }
        entry.insert("enabled".into(), Value::Bool(server.enabled));
        mcp_obj.insert(server.name.clone(), Value::Object(entry));
    }

    Ok(root)
}

/// Removes only the named servers from `mcp`, leaving anything else
/// (including `lsp`) untouched.
pub fn remove(path: &Path, names: &[String]) -> Result<Option<Value>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let stripped = strip_jsonc_comments(&text);
    let mut root: Value =
        serde_json::from_str(&stripped).with_context(|| format!("parsing {} as JSONC", path.display()))?;
    let root_obj = root.as_object_mut().context("opencode.jsonc root is not a JSON object")?;
    if let Some(mcp_obj) = root_obj.get_mut("mcp").and_then(|v| v.as_object_mut()) {
        for name in names {
            mcp_obj.remove(name);
        }
    }
    Ok(Some(root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn strips_line_and_block_comments_outside_strings() {
        let input = "{\n  // a comment\n  \"a\": \"http://not-a-comment\", /* block */ \"b\": 1\n}";
        let stripped = strip_jsonc_comments(input);
        let value: Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(value["a"], "http://not-a-comment");
        assert_eq!(value["b"], 1);
    }

    #[test]
    fn adds_server_to_fresh_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencode.jsonc");
        let servers = vec![McpServerSpec {
            name: "git".into(),
            command: "uvx".into(),
            args: vec!["mcp-server-git".into()],
            env: BTreeMap::new(),
            enabled: true,
        }];
        let result = apply(&path, &servers).unwrap();
        assert_eq!(result["mcp"]["git"]["command"][0], "uvx");
        assert_eq!(result["mcp"]["git"]["command"][1], "mcp-server-git");
        assert_eq!(result["mcp"]["git"]["type"], "local");
    }

    #[test]
    fn parses_real_jsonc_with_comments_and_preserves_lsp_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencode.jsonc");
        std::fs::write(
            &path,
            "{\n  // lsp servers\n  \"lsp\": { \"rust\": {} },\n  \"mcp\": {}\n}",
        )
        .unwrap();
        let servers = vec![McpServerSpec {
            name: "memory".into(),
            command: "npx".into(),
            args: vec![],
            env: BTreeMap::new(),
            enabled: true,
        }];
        let result = apply(&path, &servers).unwrap();
        assert!(result["lsp"]["rust"].is_object());
        assert_eq!(result["mcp"]["memory"]["type"], "local");
    }
}
