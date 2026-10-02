use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchResult {
    Added,
    AlreadyPresent,
}

/// True if a hook command in `v` points into the managed adapter directory.
fn contains_marker(v: &Value, marker: &str) -> bool {
    match v {
        Value::Array(a) => a.iter().any(|e| contains_marker(e, marker)),
        Value::Object(o) => {
            o.get("command")
                .and_then(Value::as_str)
                .is_some_and(|s| s.contains(marker))
                || o.get("hooks")
                    .is_some_and(|hooks| contains_marker(hooks, marker))
        }
        _ => false,
    }
}

/// Strip managed commands, retaining other hooks even when they share an entry.
fn strip_owned(arr: &mut Vec<Value>, marker: &str) -> bool {
    let mut removed = false;
    arr.retain_mut(|entry| {
        if let Some(hooks) = entry.get_mut("hooks").and_then(Value::as_array_mut) {
            let before = hooks.len();
            hooks.retain(|hook| !contains_marker(hook, marker));
            if hooks.len() != before {
                removed = true;
                return !hooks.is_empty();
            }
        }
        if contains_marker(entry, marker) {
            removed = true;
            false
        } else {
            true
        }
    });
    removed
}

/// Mutable reference to `settings["hooks"][key]` as an array, creating the
/// `hooks` object and the array if missing.
fn hook_array<'a>(settings: &'a mut Value, key: &str) -> &'a mut Vec<Value> {
    if !settings.is_object() {
        *settings = Value::Object(serde_json::Map::new());
    }
    let obj = settings.as_object_mut().expect("just ensured object");
    let hooks = obj
        .entry("hooks")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if !hooks.is_object() {
        *hooks = Value::Object(serde_json::Map::new());
    }
    let arr = hooks
        .as_object_mut()
        .expect("just ensured object")
        .entry(key)
        .or_insert_with(|| Value::Array(Vec::new()));
    if !arr.is_array() {
        *arr = Value::Array(Vec::new());
    }
    arr.as_array_mut().expect("just ensured array")
}

/// Insert `desired` into `settings.hooks[key]`, replacing any stale ironlint entry.
///
/// Identified by `marker`. Idempotent: if the only ironlint entry already equals
/// `desired`, returns `AlreadyPresent`.
pub fn sync_hook_array(
    settings: &mut Value,
    key: &str,
    desired: Value,
    marker: &str,
) -> PatchResult {
    let arr = hook_array(settings, key);
    let ironlint: Vec<&Value> = arr.iter().filter(|e| contains_marker(e, marker)).collect();
    if ironlint.len() == 1 && ironlint[0] == &desired {
        return PatchResult::AlreadyPresent;
    }
    strip_owned(arr, marker);
    arr.push(desired);
    PatchResult::Added
}

/// Remove every ironlint-owned entry from `settings.hooks[key]`. Returns whether
/// anything was removed.
pub fn remove_from_hook_array(settings: &mut Value, key: &str, marker: &str) -> bool {
    strip_owned(hook_array(settings, key), marker)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn claude_entry(cmd: &str) -> serde_json::Value {
        json!({"matcher": "Edit|Write",
               "hooks": [{"type": "command", "command": cmd}]})
    }

    #[test]
    fn sync_inserts_into_empty_settings() {
        let mut s = json!({});
        let cmd = "\"/h/adapters/claude-code/hook.sh\" pre-tool-use";
        let r = sync_hook_array(
            &mut s,
            "PreToolUse",
            claude_entry(cmd),
            "/h/adapters/claude-code/",
        );
        assert!(matches!(r, PatchResult::Added));
        assert_eq!(s["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn sync_is_idempotent_for_identical_entry() {
        let cmd = "\"/h/adapters/codex/hook.sh\" pre-tool-use";
        let entry = json!({"command": cmd, "match": "apply_patch|Edit|Write",
                           "description": "ironlint", "timeout": 30000});
        let mut s = json!({});
        sync_hook_array(&mut s, "PreToolUse", entry.clone(), "/h/adapters/codex/");
        let r = sync_hook_array(&mut s, "PreToolUse", entry, "/h/adapters/codex/");
        assert!(matches!(r, PatchResult::AlreadyPresent));
        assert_eq!(s["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn sync_strips_stale_ironlint_entry_and_keeps_foreign() {
        let mut s = json!({"hooks": {"PreToolUse": [
            {"command": "\"/h/adapters/codex/hook.sh\" pre-tool-use", "match": "old"},
            {"command": "other-tool guard", "match": "x"}
        ]}});
        let new_cmd = "\"/h/adapters/codex/hook.sh\" pre-tool-use";
        let entry = json!({"command": new_cmd, "match": "apply_patch|Edit|Write"});
        let r = sync_hook_array(&mut s, "PreToolUse", entry, "/h/adapters/codex/");
        assert!(matches!(r, PatchResult::Added));
        let arr = s["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(arr.len(), 2); // foreign kept, single ironlint entry refreshed
        assert!(arr.iter().any(|e| e["command"] == "other-tool guard"));
        assert!(arr.iter().any(|e| e["match"] == "apply_patch|Edit|Write"));
    }

    #[test]
    fn sync_keeps_foreign_hook_chained_in_same_entry() {
        let mut settings = json!({"hooks": {"PreToolUse": [{
            "matcher": "Edit|Write",
            "hooks": [
                {"type": "command", "command": "\"/h/adapters/claude-code/hook.sh\" pre-tool-use"},
                {"type": "command", "command": "my-custom-hook"}
            ]
        }]}});
        let desired = claude_entry("\"/h/adapters/claude-code/new-hook.sh\" pre-tool-use");
        sync_hook_array(
            &mut settings,
            "PreToolUse",
            desired,
            "/h/adapters/claude-code/",
        );
        let entries = settings["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0]["hooks"],
            json!([
                {"type": "command", "command": "my-custom-hook"}
            ])
        );
    }

    #[test]
    fn remove_drops_only_ironlint_entries() {
        let mut s = json!({"hooks": {"PreToolUse": [
            claude_entry("\"/h/adapters/claude-code/hook.sh\" pre-tool-use"),
            {"matcher": "Edit", "hooks": [{"type": "command", "command": "keep me"}]}
        ]}});
        let removed = remove_from_hook_array(&mut s, "PreToolUse", "/h/adapters/claude-code/");
        assert!(removed);
        let arr = s["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["hooks"][0]["command"], "keep me");
    }

    #[test]
    fn remove_keeps_foreign_hook_chained_in_same_entry() {
        let mut settings = json!({"hooks": {"PreToolUse": [{
            "matcher": "Edit|Write",
            "hooks": [
                {"type": "command", "command": "\"/h/adapters/claude-code/hook.sh\" pre-tool-use"},
                {"type": "command", "command": "my-custom-hook"}
            ]
        }]}});
        assert!(remove_from_hook_array(
            &mut settings,
            "PreToolUse",
            "/h/adapters/claude-code/"
        ));
        assert_eq!(
            settings["hooks"]["PreToolUse"][0]["hooks"],
            json!([
                {"type": "command", "command": "my-custom-hook"}
            ])
        );
    }

    #[test]
    fn remove_returns_false_when_absent() {
        let mut s = json!({"hooks": {"PostToolUse": []}});
        assert!(!remove_from_hook_array(
            &mut s,
            "PostToolUse",
            "/h/adapters/claude-code/"
        ));
    }

    #[test]
    fn sync_coerces_non_object_root() {
        let mut s = json!("not an object");
        sync_hook_array(&mut s, "PostToolUse", json!({"x": 1}), "marker");
        assert!(s.is_object());
        assert_eq!(s["hooks"]["PostToolUse"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn sync_coerces_non_object_hooks() {
        let mut s = json!({"hooks": "garbage"});
        sync_hook_array(&mut s, "PreToolUse", json!({"y": 2}), "marker");
        assert_eq!(s["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn sync_coerces_non_array_key_slot() {
        let mut s = json!({"hooks": {"PostToolUse": "garbage"}});
        sync_hook_array(&mut s, "PostToolUse", json!({"z": 3}), "marker");
        assert_eq!(s["hooks"]["PostToolUse"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn contains_marker_false_for_scalars() {
        assert!(!contains_marker(&json!(30000), "marker"));
        assert!(!contains_marker(&json!(true), "marker"));
        assert!(!contains_marker(&json!(null), "marker"));
    }
}
