use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use super::scope::ScopeMatcher;

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const DEFAULT_TOTAL_TIMEOUT_SECS: u64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum V1Event {
    Change,
    Accept,
}

impl V1Event {
    pub fn parse(event: &str) -> Result<Self> {
        match event {
            "change" => Ok(Self::Change),
            "accept" => Ok(Self::Accept),
            _ => Err(anyhow!(
                "invalid v1 event `{event}`; expected `change` or `accept`"
            )),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Change => "change",
            Self::Accept => "accept",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct V1Execution {
    timeout_secs: u64,
    total_timeout_secs: u64,
}
impl V1Execution {
    pub fn timeout_secs(&self) -> u64 {
        self.timeout_secs
    }
    pub fn total_timeout_secs(&self) -> u64 {
        self.total_timeout_secs
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct V1Check {
    #[serde(skip_serializing_if = "Option::is_none")]
    files: Option<Vec<String>>,
    on: Vec<V1Event>,
    run: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout_secs: Option<u64>,
    #[serde(skip)]
    matcher: Option<ScopeMatcher>,
}
impl V1Check {
    pub fn files(&self) -> Option<&[String]> {
        self.files.as_deref()
    }
    pub fn on(&self) -> &[V1Event] {
        &self.on
    }
    pub fn run(&self) -> &str {
        &self.run
    }
    /// Explicit command timeout override; `None` inherits the global default.
    pub fn timeout_secs(&self) -> Option<u64> {
        self.timeout_secs
    }
    /// Configured command budget before the remaining total deadline caps it.
    pub fn effective_timeout_secs(&self, execution: &V1Execution) -> u64 {
        self.timeout_secs.unwrap_or(execution.timeout_secs())
    }
    pub fn selection(&self, event: V1Event, paths: Option<&[PathBuf]>) -> SelectionDecision {
        if event == V1Event::Accept {
            return SelectionDecision::Acceptance;
        }
        if !self.on.contains(&V1Event::Change) {
            return SelectionDecision::ChangeDisabled;
        }
        match (&self.matcher, paths) {
            (None, _) => SelectionDecision::Unscoped,
            (_, None) => SelectionDecision::UnknownPaths,
            (Some(_), Some([])) => SelectionDecision::EmptyPaths,
            (Some(matcher), Some(paths)) => {
                if paths.iter().any(|path| matcher.matches(path)) {
                    SelectionDecision::Matched
                } else {
                    SelectionDecision::Unmatched
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionDecision {
    Acceptance,
    ChangeDisabled,
    Unscoped,
    UnknownPaths,
    EmptyPaths,
    Matched,
    Unmatched,
}
impl SelectionDecision {
    pub fn is_selected(self) -> bool {
        matches!(
            self,
            Self::Acceptance | Self::Unscoped | Self::UnknownPaths | Self::Matched
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionRow<'a> {
    id: &'a str,
    decision: SelectionDecision,
}
impl<'a> SelectionRow<'a> {
    pub fn id(self) -> &'a str {
        self.id
    }
    pub fn decision(self) -> SelectionDecision {
        self.decision
    }
}

/// A validated v1 policy. Construction compiles scopes once; fields cannot be
/// mutated or deserialized independently of validation.
///
/// ```compile_fail
/// let policy: ironlint_core::config::V1Config = serde_yaml::from_str("version: 1").unwrap();
/// ```
///
/// ```compile_fail
/// let mut policy = ironlint_core::config::parse_v1_str("version: 1\nchecks:\n  ok: {run: 'true'}\n").unwrap();
/// policy.checks.clear();
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct V1Config {
    version: u8,
    execution: V1Execution,
    checks: BTreeMap<String, V1Check>,
}
impl V1Config {
    pub fn version(&self) -> u8 {
        self.version
    }
    pub fn execution(&self) -> &V1Execution {
        &self.execution
    }
    pub fn checks(&self) -> &BTreeMap<String, V1Check> {
        &self.checks
    }
    pub fn selection(&self, event: V1Event, paths: Option<&[PathBuf]>) -> Vec<SelectionRow<'_>> {
        self.checks
            .iter()
            .map(|(id, check)| SelectionRow {
                id,
                decision: check.selection(event, paths),
            })
            .collect()
    }
    pub fn selected_ids(&self, event: V1Event, paths: Option<&[PathBuf]>) -> Vec<&str> {
        self.selection(event, paths)
            .into_iter()
            .filter(|row| row.decision.is_selected())
            .map(|row| row.id)
            .collect()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawExecution {
    #[serde(default = "default_timeout_secs")]
    timeout_secs: u64,
    #[serde(default = "default_total_timeout_secs")]
    total_timeout_secs: u64,
}
impl Default for RawExecution {
    fn default() -> Self {
        Self {
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            total_timeout_secs: DEFAULT_TOTAL_TIMEOUT_SECS,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCheck {
    #[serde(default, deserialize_with = "deserialize_files")]
    files: Option<Vec<String>>,
    #[serde(default = "default_events")]
    on: Vec<V1Event>,
    run: String,
    #[serde(default, deserialize_with = "deserialize_timeout_secs")]
    timeout_secs: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    version: u8,
    #[serde(default)]
    execution: RawExecution,
    #[serde(deserialize_with = "deserialize_checks")]
    checks: BTreeMap<String, RawCheck>,
}

pub fn parse_v1_str(input: &str) -> Result<V1Config> {
    let value =
        serde_yaml::from_str::<serde_yaml::Value>(input).context("parsing v1 config mappings")?;
    if !value
        .as_mapping()
        .is_some_and(|mapping| mapping.contains_key(serde_yaml::Value::String("version".into())))
    {
        return Err(anyhow!("unsupported unversioned config; add `version: 1` and use the v1 format shown by `ironlint schema`"));
    }
    let raw: RawConfig = serde_yaml::from_value(value).context("parsing v1 config")?;
    validate_structure(&raw)?;
    let checks = raw
        .checks
        .into_iter()
        .map(|(id, raw)| {
            let check = validate_check(&id, raw)?;
            Ok((id, check))
        })
        .collect::<Result<_>>()?;
    Ok(V1Config {
        version: raw.version,
        execution: V1Execution {
            timeout_secs: raw.execution.timeout_secs,
            total_timeout_secs: raw.execution.total_timeout_secs,
        },
        checks,
    })
}

pub fn parse_v1_file(path: &Path) -> Result<V1Config> {
    let bytes =
        std::fs::read(path).with_context(|| format!("reading v1 config {}", path.display()))?;
    parse_v1_bytes(&bytes)
}
pub fn parse_v1_bytes(bytes: &[u8]) -> Result<V1Config> {
    parse_v1_str(std::str::from_utf8(bytes).context("v1 config is not valid UTF-8")?)
}
fn validate_structure(config: &RawConfig) -> Result<()> {
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
    Ok(())
}
fn validate_check(id: &str, raw: RawCheck) -> Result<V1Check> {
    if id.is_empty() {
        return Err(anyhow!("check id must be nonempty"));
    }
    if !run_has_executable_content(&raw.run) {
        return Err(anyhow!(
            "check `{id}` run must contain an executable command"
        ));
    }
    validate_events(id, &raw.on)?;
    if raw.timeout_secs == Some(0) {
        return Err(anyhow!(
            "check `{id}` timeout_secs must be a positive integer"
        ));
    }
    let matcher = raw
        .files
        .as_ref()
        .map(|files| {
            if files.is_empty() || files.iter().any(|glob| glob.trim().is_empty()) {
                return Err(anyhow!("check `{id}` files must contain a nonempty glob"));
            }
            ScopeMatcher::new(files).with_context(|| format!("invalid files glob for check `{id}`"))
        })
        .transpose()?;
    Ok(V1Check {
        files: raw.files,
        on: raw.on,
        run: raw.run,
        timeout_secs: raw.timeout_secs,
        matcher,
    })
}

fn deserialize_timeout_secs<'de, D>(deserializer: D) -> std::result::Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    u64::deserialize(deserializer).map(Some).map_err(|error| {
        serde::de::Error::custom(format!("timeout_secs must be a positive integer: {error}"))
    })
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

fn run_has_executable_content(run: &str) -> bool {
    run.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty() && !trimmed.starts_with('#')
    })
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
) -> std::result::Result<BTreeMap<String, RawCheck>, D::Error>
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
    fn rejects_unknown_execution_fields_in_the_shared_parser() {
        for field in ["timeout_sec", "total_timeout_sec", "unexpected"] {
            let input =
                format!("version: 1\nexecution: {{{field}: 1}}\nchecks:\n  ok: {{run: 'true'}}\n");
            let error = parse_v1_str(&input).unwrap_err();
            let detail = format!("{error:#}");
            assert!(detail.contains("unknown field"), "{detail}");
            assert!(detail.contains(field), "{detail}");
        }
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
