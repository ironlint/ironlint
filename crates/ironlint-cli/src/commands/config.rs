use anyhow::Context;
use std::path::{Path, PathBuf};

pub fn resolve_config(config: &Path) -> Result<PathBuf, String> {
    if config.exists() {
        return Ok(config.to_path_buf());
    }
    let cwd = std::env::current_dir().map_err(|error| format!("resolving cwd: {error}"))?;
    resolve_config_with_cwd(config, &cwd)
}

pub(crate) fn inspect_v1_bytes(input: &str) -> anyhow::Result<ironlint_core::config::V1Config> {
    ironlint_core::config::parse_v1_str(input)
}

pub(crate) fn load_read_only(config: &Path) -> anyhow::Result<ironlint_core::config::V1Config> {
    load_read_only_with_path(config).map(|(_, config)| config)
}

pub(crate) fn load_read_only_with_path(
    config: &Path,
) -> anyhow::Result<(PathBuf, ironlint_core::config::V1Config)> {
    let canonical = config
        .canonicalize()
        .with_context(|| format!("resolving config {}", config.display()))?;
    let input = std::fs::read_to_string(&canonical)
        .with_context(|| format!("reading config {}", config.display()))?;
    Ok((canonical, inspect_v1_bytes(&input)?))
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
