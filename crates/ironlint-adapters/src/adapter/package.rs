//! Adapter bytes are runtime inputs, never evaluator build inputs.
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
pub enum Source {
    Inline(&'static str),
    Package(&'static str),
}

impl Source {
    pub fn read(self) -> Result<Vec<u8>> {
        self.read_at(&package_root())
    }

    pub fn read_at(self, root: &Path) -> Result<Vec<u8>> {
        match self {
            Self::Inline(text) => Ok(text.as_bytes().to_vec()),
            Self::Package(relative) => std::fs::read(root.join(relative)).with_context(|| {
                format!("reading adapter package {}; set IRONLINT_ADAPTERS_ROOT to the installed adapters directory", root.join(relative).display())
            }),
        }
    }
}

fn package_root() -> PathBuf {
    if let Some(root) = std::env::var_os("IRONLINT_ADAPTERS_ROOT") {
        return PathBuf::from(root);
    }
    let adjacent = std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("adapters");
    if adjacent.is_dir() || !cfg!(debug_assertions) {
        return adjacent;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../adapters")
}

pub(super) fn hook_entries(source: Source, primary: &Path) -> Result<Vec<(String, Value)>> {
    let value: Value = serde_json::from_slice(&source.read()?)?;
    let hooks = value
        .get("hooks")
        .and_then(Value::as_object)
        .context("adapter package must contain a hooks object")?;
    anyhow::ensure!(!hooks.is_empty(), "adapter package has no hooks");
    let quoted = format!("'{}'", primary.to_string_lossy().replace('\'', "'\"'\"'"));
    hooks
        .iter()
        .map(|(event, groups)| {
            anyhow::ensure!(
                matches!(event.as_str(), "PostToolUse" | "Stop"),
                "unsupported adapter event {event}"
            );
            let groups = groups.as_array().context("hook groups must be an array")?;
            anyhow::ensure!(
                groups.len() == 1,
                "expected one hook group per adapter event"
            );
            let mut entry = groups[0].clone();
            let handlers = entry
                .get_mut("hooks")
                .and_then(Value::as_array_mut)
                .context("hook group must contain handlers")?;
            anyhow::ensure!(
                handlers.len() == 1,
                "expected one command per adapter event"
            );
            let handler = &mut handlers[0];
            anyhow::ensure!(
                handler["type"] == "command",
                "adapter requires command hooks"
            );
            let command = handler["command"]
                .as_str()
                .context("hook command must be a string")?;
            let command = command
                .replace("\"${CLAUDE_PLUGIN_ROOT}/hooks/hook.sh\"", &quoted)
                .replace("\"${PLUGIN_ROOT}/hooks/hook.sh\"", &quoted);
            anyhow::ensure!(!command.contains("${"), "unresolved adapter hook command");
            handler["command"] = Value::String(command);
            Ok((event.clone(), entry))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_updates_are_read_without_recompiling() {
        let tmp = tempfile::tempdir().unwrap();
        let source = Source::Package("hook");
        assert!(source.read_at(tmp.path()).is_err());
        std::fs::write(tmp.path().join("hook"), b"first").unwrap();
        assert_eq!(source.read_at(tmp.path()).unwrap(), b"first");
        std::fs::write(tmp.path().join("hook"), b"second").unwrap();
        assert_eq!(source.read_at(tmp.path()).unwrap(), b"second");
        assert_eq!(
            Source::Inline("inline").read_at(tmp.path()).unwrap(),
            b"inline"
        );
    }

    #[test]
    fn manifest_commands_quote_paths_and_preserve_parameters() {
        let source = Source::Inline(
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"sh \"${PLUGIN_ROOT}/hooks/hook.sh\" stop","timeout":610}]}]}}"#,
        );
        let entries = hook_entries(source, Path::new("/a'$(false)/hook.sh")).unwrap();
        assert_eq!(
            entries[0].1["hooks"][0]["command"],
            "sh '/a'\"'\"'$(false)/hook.sh' stop"
        );
        assert_eq!(entries[0].1["hooks"][0]["timeout"], 610);
    }

    #[test]
    fn invalid_manifests_are_rejected_before_installation() {
        for text in [
            "bad",
            "{}",
            r#"{"hooks":{}}"#,
            r#"{"hooks":{"Other":[]}}"#,
            r#"{"hooks":{"Stop":{}}}"#,
            r#"{"hooks":{"Stop":[]}}"#,
            r#"{"hooks":{"Stop":[{}]}}"#,
            r#"{"hooks":{"Stop":[{"hooks":[]}]}}"#,
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"prompt"}]}]}}"#,
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"command"}]}]}}"#,
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"${unknown}"}]}]}}"#,
        ] {
            assert!(
                hook_entries(Source::Inline(text), Path::new("/hook")).is_err(),
                "{text}"
            );
        }
    }
}
