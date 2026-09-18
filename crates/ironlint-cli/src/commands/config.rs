use anyhow::Context;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn resolve_config(config: &Path) -> Result<PathBuf, String> {
    if config.exists() {
        return Ok(config.to_path_buf());
    }
    let cwd = std::env::current_dir().map_err(|error| format!("resolving cwd: {error}"))?;
    resolve_config_with_cwd(config, &cwd)
}

#[derive(Debug)]
pub(crate) enum ReadOnlyConfig {
    V1(V1InspectionConfig),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct V1InspectionConfig {
    version: u8,
    #[serde(default, rename = "execution")]
    _execution: V1InspectionExecution,
    pub(crate) checks: BTreeMap<String, V1InspectionCheck>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1InspectionExecution {
    #[serde(default, rename = "timeout_secs")]
    _timeout_secs: u64,
    #[serde(default, rename = "total_timeout_secs")]
    _total_timeout_secs: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct V1InspectionCheck {
    #[serde(default, deserialize_with = "deserialize_v1_files")]
    pub(crate) files: Option<Vec<String>>,
    #[serde(default = "default_v1_events")]
    pub(crate) on: Vec<String>,
    pub(crate) run: String,
}

fn default_v1_events() -> Vec<String> {
    vec!["accept".to_string()]
}

fn deserialize_v1_files<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
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

pub(crate) fn inspect_v1_bytes(input: &str) -> anyhow::Result<V1InspectionConfig> {
    ironlint_core::config::validate_v1_str(input)?;
    let parsed: V1InspectionConfig =
        serde_yaml::from_str(input).context("parsing v1 inspection config")?;
    debug_assert_eq!(parsed.version, 1);
    Ok(parsed)
}

pub(crate) fn load_read_only(config: &Path) -> anyhow::Result<ReadOnlyConfig> {
    load_read_only_with_path(config).map(|(_, config)| config)
}

pub(crate) fn load_read_only_with_path(config: &Path) -> anyhow::Result<(PathBuf, ReadOnlyConfig)> {
    let canonical = config
        .canonicalize()
        .with_context(|| format!("resolving config {}", config.display()))?;
    let input = std::fs::read_to_string(&canonical)
        .with_context(|| format!("reading config {}", config.display()))?;
    Ok((canonical, ReadOnlyConfig::V1(inspect_v1_bytes(&input)?)))
}

fn resolve_config_with_cwd(config: &Path, cwd: &Path) -> Result<PathBuf, String> {
    if config.is_absolute() {
        return Err(format!(
            "no config found at {} — run `ironlint init`",
            config.display()
        ));
    }
    let name = config
        .file_name()
        .ok_or_else(|| format!("invalid config path: {}", config.display()))?;
    let mut dir = cwd;
    loop {
        let candidate = dir.join(name);
        if candidate.exists() {
            return Ok(candidate);
        }
        if dir.join(".git").exists() {
            return Err(format!(
                "no `{}` found in {} or any parent — run `ironlint init`",
                name.to_string_lossy(),
                cwd.display()
            ));
        }
        let Some(parent) = dir.parent() else {
            return Err(format!(
                "no `{}` found in {} or any parent — run `ironlint init`",
                name.to_string_lossy(),
                cwd.display()
            ));
        };
        dir = parent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_inspection_rejects_invalid_exact_bytes() {
        let error = inspect_v1_bytes(
            "version: 1\nchecks:\n  feedback:\n    on: [change]\n    run: exit 0\n",
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("must include accept"),
            "{error:#}"
        );
    }
}
