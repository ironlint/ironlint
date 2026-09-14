use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Deserialize;
use std::collections::BTreeMap;

/// Resolve the `--config` value to an actual file path. If `config` is the
/// default (`.ironlint.yml`) and doesn't exist in cwd, walk up the directory
/// tree (stopping at a `.git` directory or the filesystem root) looking for
/// one. If `config` was explicitly passed (even if it's the default name but
/// the caller intends a specific file), the caller is responsible — but clap's
/// default-value mechanism can't distinguish "user typed --config .ironlint.yml"
/// from "user omitted --config". So: walk-up applies unconditionally when the
/// literal path doesn't exist in cwd.
///
/// Returns the resolved path, or an error with an actionable message naming
/// `ironlint init` when no config is found in cwd or any parent.
pub fn resolve_config(config: &Path) -> Result<PathBuf, String> {
    if config.exists() {
        return Ok(config.to_path_buf());
    }
    let cwd = std::env::current_dir().map_err(|e| format!("resolving cwd: {e}"))?;
    resolve_config_with_cwd(config, &cwd)
}

pub(crate) fn is_versioned_config(config: &Path) -> bool {
    let Ok(bytes) = std::fs::read(config) else {
        return false;
    };
    match std::str::from_utf8(&bytes) {
        Ok(input) => is_versioned_input(input),
        Err(error) => {
            std::str::from_utf8(&bytes[..error.valid_up_to()]).is_ok_and(is_versioned_input)
        }
    }
}

fn is_versioned_input(input: &str) -> bool {
    if let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(input) {
        return value.as_mapping().is_some_and(|mapping| {
            mapping.contains_key(serde_yaml::Value::String("version".into()))
        });
    }
    let mut prefix = String::new();
    for line in input.lines() {
        let trimmed = line.trim_start();
        if let Some((key, _)) = trimmed.split_once(':') {
            if matches!(key.trim(), "version" | "\"version\"" | "'version'") {
                let mut candidate = prefix.clone();
                candidate.push_str(&line[..line.len() - trimmed.len()]);
                candidate.push_str(key);
                candidate.push_str(": 1\n");
                if serde_yaml::from_str::<serde_yaml::Value>(&candidate).is_ok_and(|value| {
                    value.as_mapping().is_some_and(|mapping| {
                        mapping.contains_key(serde_yaml::Value::String("version".into()))
                    })
                }) {
                    return true;
                }
            }
        }
        prefix.push_str(line);
        prefix.push('\n');
    }
    malformed_root_flow_has_version(input) || malformed_root_version_by_indent(input)
}

fn malformed_root_version_by_indent(input: &str) -> bool {
    let mut shallowest = usize::MAX;
    let mut has_version = false;
    let mut flow_depth = 0;
    let mut quote = None;
    let mut can_quote = true;
    let mut pending_flow = false;
    for line in input.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if flow_depth > 0 {
            advance_flow_collection(trimmed, &mut flow_depth, &mut quote, &mut can_quote);
            continue;
        }
        if pending_flow && matches!(trimmed.chars().next(), Some('[' | '{')) {
            pending_flow = false;
            can_quote = true;
            advance_flow_collection(trimmed, &mut flow_depth, &mut quote, &mut can_quote);
            continue;
        }
        pending_flow = false;
        let indent = line.len() - trimmed.len();
        if trimmed
            .strip_prefix("? ")
            .is_some_and(malformed_version_name)
        {
            if indent < shallowest {
                shallowest = indent;
                has_version = false;
            }
            if indent == shallowest {
                has_version = true;
            }
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        if indent < shallowest {
            shallowest = indent;
            has_version = false;
        }
        if indent == shallowest && matches!(key.trim(), "version" | "\"version\"" | "'version'") {
            has_version = true;
        }
        let flow_token = value
            .split_whitespace()
            .take_while(|token| !token.starts_with('#'))
            .find(|token| !token.starts_with('&') && !token.starts_with('!'));
        if flow_token.is_some_and(|token| token.starts_with('[') || token.starts_with('{')) {
            can_quote = true;
            advance_flow_collection(value, &mut flow_depth, &mut quote, &mut can_quote);
        } else if flow_token.is_none()
            && matches!(value.trim_start().chars().next(), Some('&' | '!'))
        {
            pending_flow = true;
        }
    }
    has_version
}

fn malformed_root_flow_has_version(input: &str) -> bool {
    let mut offset = 0;
    for line in input.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            if trimmed
                .strip_prefix("---")
                .is_some_and(|rest| rest.trim().is_empty() || rest.trim_start().starts_with('#'))
            {
                offset += line.len();
                continue;
            }
            let input = &input[offset + line.len() - trimmed.len()..];
            let input = input.strip_prefix("---").unwrap_or(input).trim_start();
            return input
                .strip_prefix('{')
                .is_some_and(malformed_flow_mapping_has_version);
        }
        offset += line.len();
    }
    false
}

fn malformed_flow_mapping_has_version(input: &str) -> bool {
    let mut depth = 1usize;
    let mut quote = None;
    let mut escaped = false;
    let mut previous = ' ';
    let mut in_comment = false;
    let mut key_start = 0;
    let mut looking_for_key = true;
    for (index, ch) in input.char_indices() {
        if in_comment {
            if ch == '\n' {
                in_comment = false;
                if looking_for_key {
                    key_start = index + ch.len_utf8();
                }
            }
            previous = ch;
            continue;
        }
        if let Some(delimiter) = quote {
            if delimiter == '"' && ch == '\\' && !escaped {
                escaped = true;
            } else {
                if ch == delimiter && !escaped {
                    quote = None;
                }
                escaped = false;
            }
            previous = ch;
            continue;
        }
        match ch {
            '#' if previous.is_whitespace() => in_comment = true,
            '\'' | '"' => quote = Some(ch),
            '[' | '{' => depth += 1,
            ']' | '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return false;
                }
            }
            ':' if depth == 1 && looking_for_key => {
                if malformed_version_name(&input[key_start..index]) {
                    return true;
                }
                looking_for_key = false;
            }
            ',' if depth == 1 => {
                key_start = index + ch.len_utf8();
                looking_for_key = true;
            }
            _ => {}
        }
        previous = ch;
    }
    false
}

fn malformed_version_name(input: &str) -> bool {
    matches!(input.trim(), "version" | "\"version\"" | "'version'")
}

fn advance_flow_collection(
    line: &str,
    depth: &mut usize,
    quote: &mut Option<char>,
    can_quote: &mut bool,
) {
    let mut escaped = false;
    let mut previous = ' ';
    for ch in line.chars() {
        if let Some(delimiter) = *quote {
            if delimiter == '"' && ch == '\\' && !escaped {
                escaped = true;
            } else {
                if ch == delimiter && !escaped {
                    *quote = None;
                }
                escaped = false;
            }
            previous = ch;
            continue;
        }
        match ch {
            '#' if previous.is_whitespace() => break,
            '\'' | '"' if *can_quote => *quote = Some(ch),
            '[' | '{' => {
                *depth += 1;
                *can_quote = true;
            }
            ']' | '}' => {
                *depth = depth.saturating_sub(1);
                *can_quote = false;
            }
            ',' | ':' => *can_quote = true,
            _ if !ch.is_whitespace() => *can_quote = false,
            _ => {}
        }
        previous = ch;
    }
}

#[derive(Debug)]
pub(crate) enum ReadOnlyConfig {
    Legacy {
        config: ironlint_core::config::Config,
        origins: BTreeMap<String, PathBuf>,
    },
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
    if is_versioned_input(&input) {
        let parsed = inspect_v1_bytes(&input)?;
        return Ok((canonical, ReadOnlyConfig::V1(parsed)));
    }
    let (config, origins) =
        ironlint_core::config::extends::resolve_with_origin_from_str(&canonical, &input)?;
    Ok((canonical, ReadOnlyConfig::Legacy { config, origins }))
}

fn resolve_config_with_cwd(config: &Path, cwd: &Path) -> Result<PathBuf, String> {
    // An absolute path is unambiguously explicit; don't fall back to a parent
    // directory. Only walk up for relative paths (the clap default `.ironlint.yml`
    // plus any explicit relative config name).
    if config.is_absolute() {
        return Err(format!(
            "no config found at {} — run `ironlint init`",
            config.display()
        ));
    }
    let name = config
        .file_name()
        .ok_or_else(|| format!("invalid config path: {}", config.display()))?;
    let mut dir: &Path = cwd;
    loop {
        let candidate = dir.join(name);
        if candidate.exists() {
            return Ok(candidate);
        }
        // Stop at the repo boundary.
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
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn nested_version_marker_does_not_select_v1() {
        assert!(!is_versioned_input("checks:\n  version: 1\n  broken: [\n"));
        assert!(!is_versioned_input("checks: [\nversion: 1\n"));
        assert!(!is_versioned_input("checks: [\"]\"\nversion: 1\n"));
        assert!(is_versioned_input(
            "checks: [\"[\"]\nversion: 1\nbroken: [\n"
        ));
        assert!(is_versioned_input(
            "checks: [it's]\nversion: 1\nbroken: [\n"
        ));
        assert!(is_versioned_input(
            "checks: [foo#bar]\nversion: 1\nbroken: [\n"
        ));
        assert!(!is_versioned_input("checks: &a [\nversion: 1\n"));
        assert!(!is_versioned_input("checks: !!seq [\nversion: 1\n"));
        assert!(is_versioned_input(
            "checks: [foo\n'bar]\nversion: 1\nbroken: [\n"
        ));
        assert!(!is_versioned_input("checks: &a\n  [\nversion: 1\n"));
        assert!(!is_versioned_input("checks: &a # note\n  [\nversion: 1\n"));
        assert!(!is_versioned_input("{checks: {version: 1}, broken: ["));
        assert!(!is_versioned_input("{checks: \"version: 1\", broken: ["));
        assert!(is_versioned_input("checks: [a,,b]\nversion: 1\n"));
    }

    #[test]
    fn v1_inspection_rejects_invalid_exact_bytes() {
        let err = inspect_v1_bytes(
            "version: 1\nchecks:\n  feedback:\n    on: [change]\n    run: exit 0\n",
        )
        .unwrap_err();
        assert!(err.to_string().contains("must include accept"), "{err:#}");
    }

    #[test]
    fn relative_found_in_cwd() {
        let tmp = tempdir().unwrap();
        let cfg = tmp.path().join(".ironlint.yml");
        fs::write(&cfg, "checks:\n").unwrap();
        let resolved = resolve_config_with_cwd(Path::new(".ironlint.yml"), tmp.path()).unwrap();
        assert_eq!(
            resolved.file_name(),
            Some(std::ffi::OsStr::new(".ironlint.yml"))
        );
    }

    #[test]
    fn relative_found_in_parent() {
        let tmp = tempdir().unwrap();
        let cfg = tmp.path().join(".ironlint.yml");
        fs::write(&cfg, "checks:\n").unwrap();
        let subdir = tmp.path().join("src/nested");
        fs::create_dir_all(&subdir).unwrap();
        let resolved = resolve_config_with_cwd(Path::new(".ironlint.yml"), &subdir).unwrap();
        assert_eq!(
            resolved.canonicalize().unwrap(),
            cfg.canonicalize().unwrap()
        );
    }

    #[test]
    fn git_boundary_stops_walk() {
        let tmp = tempdir().unwrap();
        fs::create_dir_all(tmp.path().join(".git")).unwrap();
        let subdir = tmp.path().join("deep");
        fs::create_dir_all(&subdir).unwrap();
        let err = resolve_config_with_cwd(Path::new(".ironlint.yml"), &subdir).unwrap_err();
        assert!(err.contains("ironlint init"), "{err}");
        assert!(err.contains(".ironlint.yml"), "{err}");
    }

    #[test]
    fn absolute_missing_is_error() {
        let err = resolve_config_with_cwd(
            Path::new("/definitely/not/a/real/.ironlint.yml"),
            Path::new("/"),
        )
        .unwrap_err();
        assert!(err.contains("ironlint init"), "{err}");
    }

    #[test]
    fn invalid_empty_config_path_is_error() {
        let err = resolve_config_with_cwd(Path::new(""), Path::new("/")).unwrap_err();
        assert!(err.contains("invalid config path"), "{err}");
    }
}
