use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Deserializer};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use super::scope::ScopeMatcher;

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const DEFAULT_TOTAL_TIMEOUT_SECS: u64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum V1Event {
    Change,
    Accept,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct V1Execution {
    #[serde(default = "default_timeout_secs")]
    pub(crate) timeout_secs: u64,
    #[serde(default = "default_total_timeout_secs")]
    pub(crate) total_timeout_secs: u64,
}

impl Default for V1Execution {
    fn default() -> Self {
        Self {
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            total_timeout_secs: DEFAULT_TOTAL_TIMEOUT_SECS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct V1Check {
    #[serde(default, deserialize_with = "deserialize_files")]
    pub(crate) files: Option<Vec<String>>,
    #[serde(default = "default_events")]
    pub(crate) on: Vec<V1Event>,
    pub(crate) run: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct V1Config {
    pub(crate) version: u8,
    #[serde(default)]
    pub(crate) execution: V1Execution,
    #[serde(deserialize_with = "deserialize_checks")]
    pub(crate) checks: BTreeMap<String, V1Check>,
}

impl V1Config {
    pub(crate) fn selected_ids(
        &self,
        event: V1Event,
        changed_paths: Option<&[PathBuf]>,
    ) -> Vec<&str> {
        self.checks
            .iter()
            .filter(|(_, check)| selects(check, event, changed_paths))
            .map(|(id, _)| id.as_str())
            .collect()
    }
}

fn selects(check: &V1Check, event: V1Event, changed_paths: Option<&[PathBuf]>) -> bool {
    if event == V1Event::Accept {
        return true;
    }
    if !check.on.contains(&V1Event::Change) {
        return false;
    }
    match (check.files.as_deref(), changed_paths) {
        (None, _) | (_, None) => true,
        (Some(_), Some([])) => false,
        (Some(globs), Some(paths)) => ScopeMatcher::new(globs)
            .map(|matcher| paths.iter().any(|path| matcher.matches(path)))
            .unwrap_or(false),
    }
}

pub(crate) fn parse_v1_str(input: &str) -> Result<V1Config> {
    serde_yaml::from_str::<serde_yaml::Value>(input).context("parsing v1 config mappings")?;
    let config: V1Config = serde_yaml::from_str(input).context("parsing v1 config")?;
    validate(&config)?;
    Ok(config)
}

pub(crate) fn parse_v1_file(path: &Path) -> Result<V1Config> {
    let input = std::fs::read_to_string(path)
        .with_context(|| format!("reading v1 config {}", path.display()))?;
    parse_v1_str(&input)
}

fn validate(config: &V1Config) -> Result<()> {
    if config.version != 1 {
        return Err(anyhow!(
            "unsupported config version {}; expected version: 1",
            config.version
        ));
    }
    if config.checks.is_empty() {
        return Err(anyhow!("checks must be a nonempty mapping"));
    }
    if config.execution.timeout_secs == 0 || config.execution.total_timeout_secs == 0 {
        return Err(anyhow!("execution budgets must be positive integers"));
    }
    for (id, check) in &config.checks {
        if id.is_empty() {
            return Err(anyhow!("check id must be nonempty"));
        }
        if !super::parser::run_has_executable_content(&check.run) {
            return Err(anyhow!(
                "check `{id}` run must contain an executable command"
            ));
        }
        validate_events(id, &check.on)?;
        if let Some(files) = &check.files {
            if files.is_empty() || files.iter().any(|glob| glob.trim().is_empty()) {
                return Err(anyhow!("check `{id}` files must contain a nonempty glob"));
            }
            ScopeMatcher::new(files)
                .with_context(|| format!("invalid files glob for check `{id}`"))?;
        }
    }
    Ok(())
}

fn validate_events(id: &str, events: &[V1Event]) -> Result<()> {
    let mut seen = HashSet::new();
    if events.is_empty() || !events.contains(&V1Event::Accept) {
        return Err(anyhow!("check `{id}` on must include accept"));
    }
    for event in events {
        if !seen.insert(*event) {
            return Err(anyhow!("check `{id}` on contains duplicate events"));
        }
    }
    Ok(())
}

fn deserialize_files<'de, D>(deserializer: D) -> std::result::Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_yaml::Value::deserialize(deserializer)?;
    match value {
        serde_yaml::Value::String(glob) => Ok(Some(vec![glob])),
        serde_yaml::Value::Sequence(globs) => globs
            .into_iter()
            .map(|glob| {
                glob.as_str()
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| serde::de::Error::custom("files entries must be strings"))
            })
            .collect::<std::result::Result<Vec<_>, _>>()
            .map(Some),
        _ => Err(serde::de::Error::custom(
            "files must be a glob string or list of strings",
        )),
    }
}

fn deserialize_checks<'de, D>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, V1Check>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_yaml::Value::deserialize(deserializer)?;
    let mapping = value
        .as_mapping()
        .ok_or_else(|| serde::de::Error::custom("checks must be a mapping"))?;
    mapping
        .iter()
        .map(|(key, value)| {
            let id = key
                .as_str()
                .ok_or_else(|| serde::de::Error::custom("check ids must be strings"))?;
            let check = serde_yaml::from_value(value.clone()).map_err(serde::de::Error::custom)?;
            Ok((id.to_owned(), check))
        })
        .collect()
}

fn default_events() -> Vec<V1Event> {
    vec![V1Event::Accept]
}

fn default_timeout_secs() -> u64 {
    DEFAULT_TIMEOUT_SECS
}

fn default_total_timeout_secs() -> u64 {
    DEFAULT_TOTAL_TIMEOUT_SECS
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(input: &str) -> Result<V1Config> {
        parse_v1_str(input)
    }

    #[test]
    fn parses_v1_defaults_and_selects_acceptance() {
        let config = parse("version: 1\nchecks:\n  tests:\n    run: ./scripts/test\n").unwrap();
        assert_eq!(config.execution.timeout_secs, 30);
        assert_eq!(config.execution.total_timeout_secs, 300);
        assert_eq!(config.checks["tests"].on, vec![V1Event::Accept]);
        assert_eq!(config.selected_ids(V1Event::Accept, None), vec!["tests"]);
    }

    #[test]
    fn parses_change_check_files_and_explicit_budgets() {
        let config = parse(
            "version: 1\nexecution:\n  timeout_secs: 1\n  total_timeout_secs: 2\nchecks:\n  lint:\n    files: [src/**, '*.rs']\n    on: [change, accept]\n    run: lint\n",
        )
        .unwrap();
        assert_eq!(config.execution.timeout_secs, 1);
        assert_eq!(config.execution.total_timeout_secs, 2);
        assert_eq!(config.checks["lint"].files.as_ref().unwrap().len(), 2);
        let paths = [PathBuf::from("src/a.c")];
        assert_eq!(
            config.selected_ids(V1Event::Change, Some(&paths)),
            vec!["lint"]
        );
    }

    #[test]
    fn acceptance_ignores_files_and_on() {
        let config = parse(
            "version: 1\nchecks:\n  accept_only:\n    run: 'true'\n  change:\n    files: missing/**\n    on: [change, accept]\n    run: 'true'\n",
        )
        .unwrap();
        assert_eq!(
            config.selected_ids(V1Event::Accept, Some(&[])),
            vec!["accept_only", "change"]
        );
    }

    #[test]
    fn change_selection_distinguishes_unknown_empty_and_matching_paths() {
        let config = parse(
            "version: 1\nchecks:\n  unconditional:\n    on: [change, accept]\n    run: 'true'\n  scoped:\n    files: src/**\n    on: [change, accept]\n    run: 'true'\n  accept_only:\n    run: 'true'\n",
        )
        .unwrap();
        assert_eq!(
            config.selected_ids(V1Event::Change, None),
            vec!["scoped", "unconditional"]
        );
        assert_eq!(
            config.selected_ids(V1Event::Change, Some(&[])),
            vec!["unconditional"]
        );
        let paths = [PathBuf::from("src/lib.rs"), PathBuf::from("src/lib.rs")];
        assert_eq!(
            config.selected_ids(V1Event::Change, Some(&paths)),
            vec!["scoped", "unconditional"]
        );
        let other = [PathBuf::from("docs/readme.md")];
        assert_eq!(
            config.selected_ids(V1Event::Change, Some(&other)),
            vec!["unconditional"]
        );
    }

    #[test]
    fn rejects_invalid_required_and_strict_fields() {
        for input in [
            "checks: {}\n",
            "version: 2\nchecks:\n  x:\n    run: 'true'\n",
            "version: 1\nchecks:\n  x:\n    on: [change]\n    run: 'true'\n",
            "version: 1\nchecks:\n  x:\n    on: [accept, accept]\n    run: 'true'\n",
            "version: 1\nchecks:\n  x:\n    files: []\n    run: 'true'\n",
            "version: 1\nchecks:\n  x:\n    files: null\n    run: 'true'\n",
            "version: 1\nchecks:\n  1:\n    run: 'true'\n  \"1\":\n    run: 'true'\n",
            "version: 1\nchecks:\n  x:\n    run: '   '\n",
            "version: 1\nexecution:\n  timeout_secs: 0\nchecks:\n  x:\n    run: true\n",
            "version: 1\nchecks:\n  x:\n    run: 'true'\n    extra: true\n",
            "version: 1\nextra: true\nchecks:\n  x:\n    run: 'true'\n",
        ] {
            assert!(parse(input).is_err(), "accepted invalid config: {input}");
        }
    }

    #[test]
    fn rejects_comment_only_run() {
        let err =
            parse("version: 1\nchecks:\n  safety:\n    run: '# TODO: implement safety check'\n")
                .unwrap_err();
        assert!(err.to_string().contains("safety"), "{err:#}");
    }

    #[test]
    fn rejects_duplicate_mapping_keys_at_each_level() {
        for input in [
            "version: 1\nversion: 1\nchecks:\n  x:\n    run: 'true'\n",
            "version: 1\nexecution:\n  timeout_secs: 1\n  timeout_secs: 2\nchecks:\n  x:\n    run: 'true'\n",
            "version: 1\nchecks:\n  x:\n    run: 'true'\n  x:\n    run: 'false'\n",
            "version: 1\nchecks:\n  x:\n    run: 'true'\n    run: 'false'\n",
        ] {
            assert!(parse(input).is_err(), "accepted duplicate mapping key: {input}");
        }
    }
}
