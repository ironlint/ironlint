use crate::adapter::materialize::{atomic_write, backup_once, read_sidecar, sha256_hex};
use crate::adapter::ownership::{install_files, remove_owned_files};
use crate::adapter::plan::PlanStep;
use crate::adapter::registry::{JsonHookSpec, PluginSpec, SkillSpec};
use crate::adapter::SKILL_NAME;
use crate::adapter::{
    adapters_dir, remove_from_hook_array, sync_hook_array, AdapterEnv, Harness, HarnessKind,
    PatchResult, Scope,
};
use crate::filesystem::ResourceLocks;
use anyhow::{Context, Result};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum InstallResult {
    Installed,
    AlreadyPresent,
    Updated,
    Skipped(String),
    Failed(String),
}

pub struct InstallOutcome {
    pub harness: &'static str,
    pub result: InstallResult,
    pub hint: &'static str,
}

pub struct HarnessStatus {
    pub harness: &'static str,
    pub detected: bool,
    pub installed: bool,
    pub registered: bool,
    pub intact: Option<bool>,
    pub current: Option<bool>,
}

/// Physical resources inspected for one adapter scope.
///
/// Local scopes may use a global fallback; consumers can distinguish a shared
/// artifact from separate settings registrations using the registry's paths.
pub struct StatusPaths {
    pub artifact: PathBuf,
    pub registration: Option<PathBuf>,
}

pub fn status_paths(h: &Harness, env: &AdapterEnv, scope: Scope) -> StatusPaths {
    match &h.kind {
        HarnessKind::JsonHook(spec) => StatusPaths {
            artifact: adapters_dir(env).join(h.name).join(spec.primary),
            registration: Some(settings_path(spec, env, scope)),
        },
        HarnessKind::Plugin(spec) => StatusPaths {
            artifact: plugin_dir(spec, env, scope).join(spec.filename),
            registration: None,
        },
    }
}

/// Read a JSON settings file, defaulting to `{}` when absent.
fn load_settings(path: &Path) -> Result<Value> {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).with_context(|| format!("parsing {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Value::Object(Map::default())),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

fn write_settings(path: &Path, value: &Value) -> Result<()> {
    backup_once(path)?;
    let json = serde_json::to_string_pretty(value)?;
    atomic_write(path, json.as_bytes())
}

fn settings_path(spec: &JsonHookSpec, env: &AdapterEnv, scope: Scope) -> PathBuf {
    match scope {
        Scope::Local => (spec.settings_local)(env).unwrap_or_else(|| (spec.settings_global)(env)),
        Scope::Global => (spec.settings_global)(env),
    }
}

fn plugin_dir(spec: &PluginSpec, env: &AdapterEnv, scope: Scope) -> PathBuf {
    let (primary, fallback) = match scope {
        Scope::Local => ((spec.dir_local)(env), (spec.dir_global)(env)),
        Scope::Global => ((spec.dir_global)(env), (spec.dir_local)(env)),
    };
    primary
        .or(fallback)
        .expect("every plugin harness has at least one dir")
}

// --- install -----------------------------------------------------------------

pub fn install(h: &Harness, env: &AdapterEnv, scope: Scope) -> Result<InstallOutcome> {
    if !h.installable {
        return Ok(InstallOutcome {
            harness: h.name,
            result: InstallResult::Skipped(
                "legacy adapter is available only for owned-install cleanup".into(),
            ),
            hint: h.restart_hint,
        });
    }
    let result = match &h.kind {
        HarnessKind::JsonHook(spec) => install_jsonhook(h.name, spec, env, scope)?,
        HarnessKind::Plugin(spec) => install_plugin(spec, env, scope)?,
    };
    Ok(InstallOutcome {
        harness: h.name,
        result,
        hint: h.restart_hint,
    })
}

fn install_jsonhook(
    name: &str,
    spec: &JsonHookSpec,
    env: &AdapterEnv,
    scope: Scope,
) -> Result<InstallResult> {
    let dir = adapters_dir(env).join(name);
    let primary_path = dir.join(spec.primary);
    let command = format!("\"{}\" {}", primary_path.display(), spec.entry_arg);
    let marker = format!("{}", dir.display());
    let settings = settings_path(spec, env, scope);
    let _locks = ResourceLocks::acquire(&[&dir, &settings])?;
    let sources: Vec<_> = spec
        .files
        .iter()
        .map(|(name, bytes)| (*name, bytes.as_bytes()))
        .collect();
    if let skipped @ InstallResult::Skipped(_) = install_files(&dir, &sources, true)? {
        return Ok(skipped);
    }

    let mut value = load_settings(&settings)?;
    let entry = (spec.build_entry)(&command);
    let patch = sync_hook_array(&mut value, spec.array_key, entry, &marker);
    Ok(match patch {
        PatchResult::AlreadyPresent => InstallResult::AlreadyPresent,
        PatchResult::Added => {
            write_settings(&settings, &value)?;
            InstallResult::Installed
        }
    })
}

fn install_plugin(spec: &PluginSpec, env: &AdapterEnv, scope: Scope) -> Result<InstallResult> {
    let dir = plugin_dir(spec, env, scope);
    let _locks = ResourceLocks::acquire(&[&dir])?;
    install_files(&dir, &[(spec.filename, spec.source.as_bytes())], false)
}

fn skill_base(spec: &SkillSpec, env: &AdapterEnv, scope: Scope) -> PathBuf {
    match scope {
        Scope::Local => (spec.dir_local)(env),
        Scope::Global => (spec.dir_global)(env),
    }
}

pub fn install_skill(h: &Harness, env: &AdapterEnv, scope: Scope) -> Result<InstallOutcome> {
    if !h.installable {
        return Ok(InstallOutcome {
            harness: h.name,
            result: InstallResult::Skipped(
                "legacy adapter is available only for owned-install cleanup".into(),
            ),
            hint: h.restart_hint,
        });
    }
    let dir = skill_base(&h.skill, env, scope).join(SKILL_NAME);
    let file = dir.join("SKILL.md");
    let _locks = ResourceLocks::acquire(&[&dir])?;
    let result = install_skill_file(&file, &dir, h.skill.source.as_bytes())?;
    Ok(InstallOutcome {
        harness: h.name,
        result,
        hint: h.restart_hint,
    })
}

fn install_skill_file(file: &Path, dir: &Path, bytes: &[u8]) -> Result<InstallResult> {
    debug_assert_eq!(file, dir.join("SKILL.md"));
    install_files(dir, &[("SKILL.md", bytes)], false)
}

pub fn uninstall_skill(h: &Harness, env: &AdapterEnv, scope: Scope) -> Result<InstallOutcome> {
    let dir = skill_base(&h.skill, env, scope).join(SKILL_NAME);
    let _locks = ResourceLocks::acquire(&[&dir])?;
    let result = remove_owned_files(&dir, &["SKILL.md"])?;
    Ok(InstallOutcome {
        harness: h.name,
        result,
        hint: h.restart_hint,
    })
}

// --- plan (preview; writes nothing) ------------------------------------------

/// Full onboarding footprint for `h`: hook/plugin file(s), the settings patch
/// (JsonHook only), and the authoring skill. Computes paths only — no I/O.
pub fn plan_install(h: &Harness, env: &AdapterEnv, scope: Scope) -> Vec<PlanStep> {
    let mut steps = match &h.kind {
        HarnessKind::JsonHook(spec) => {
            let dir = adapters_dir(env).join(h.name);
            let mut v: Vec<PlanStep> = spec
                .files
                .iter()
                .map(|(f, _)| PlanStep::Hook { path: dir.join(f) })
                .collect();
            v.push(PlanStep::Patch {
                path: settings_path(spec, env, scope),
                key: spec.array_key,
            });
            v
        }
        HarnessKind::Plugin(spec) => vec![PlanStep::Plugin {
            path: plugin_dir(spec, env, scope).join(spec.filename),
        }],
    };
    let skill_dir = skill_base(&h.skill, env, scope).join(SKILL_NAME);
    steps.push(PlanStep::Skill {
        path: skill_dir.join("SKILL.md"),
    });
    steps
}

/// Removal footprint for `h`: the adapter dir (JsonHook) or plugin file
/// (Plugin), the settings patch (JsonHook), and the skill directory.
pub fn plan_uninstall(h: &Harness, env: &AdapterEnv, scope: Scope) -> Vec<PlanStep> {
    let mut steps = match &h.kind {
        HarnessKind::JsonHook(spec) => vec![
            PlanStep::Hook {
                path: adapters_dir(env).join(h.name),
            },
            PlanStep::Patch {
                path: settings_path(spec, env, scope),
                key: spec.array_key,
            },
        ],
        HarnessKind::Plugin(spec) => vec![PlanStep::Plugin {
            path: plugin_dir(spec, env, scope).join(spec.filename),
        }],
    };
    steps.push(PlanStep::Skill {
        path: skill_base(&h.skill, env, scope).join(SKILL_NAME),
    });
    steps
}

// --- uninstall ---------------------------------------------------------------

pub fn uninstall(h: &Harness, env: &AdapterEnv, scope: Scope) -> Result<InstallOutcome> {
    let result = match &h.kind {
        HarnessKind::JsonHook(spec) => uninstall_jsonhook(h.name, spec, env, scope)?,
        HarnessKind::Plugin(spec) => uninstall_plugin(h.name, spec, env, scope)?,
    };
    Ok(InstallOutcome {
        harness: h.name,
        result,
        hint: h.restart_hint,
    })
}

fn uninstall_jsonhook(
    name: &str,
    spec: &JsonHookSpec,
    env: &AdapterEnv,
    scope: Scope,
) -> Result<InstallResult> {
    let dir = adapters_dir(env).join(name);
    let settings = settings_path(spec, env, scope);
    let _locks = ResourceLocks::acquire(&[&dir, &settings])?;
    if settings.exists() {
        let mut value = load_settings(&settings)?;
        if remove_from_hook_array(&mut value, spec.array_key, &format!("{}", dir.display())) {
            write_settings(&settings, &value)?;
        }
    }
    let names: Vec<&str> = spec.files.iter().map(|(name, _)| *name).collect();
    remove_owned_files(&dir, &names)
}

fn uninstall_plugin(
    _name: &str,
    spec: &PluginSpec,
    env: &AdapterEnv,
    scope: Scope,
) -> Result<InstallResult> {
    let dir = plugin_dir(spec, env, scope);
    let _locks = ResourceLocks::acquire(&[&dir])?;
    remove_owned_files(&dir, &[spec.filename])
}

// --- status ------------------------------------------------------------------

pub fn status(h: &Harness, env: &AdapterEnv, scope: Scope) -> Result<HarnessStatus> {
    let detected = crate::adapter::registry::is_detected(h, env);
    let (installed, registered, intact, current) = match &h.kind {
        HarnessKind::JsonHook(spec) => status_jsonhook(h.name, spec, env, scope, detected)?,
        HarnessKind::Plugin(spec) => status_plugin(spec, env, scope)?,
    };
    Ok(HarnessStatus {
        harness: h.name,
        detected,
        installed,
        registered,
        intact,
        current,
    })
}

fn status_jsonhook(
    name: &str,
    spec: &JsonHookSpec,
    env: &AdapterEnv,
    scope: Scope,
    _detected: bool,
) -> Result<(bool, bool, Option<bool>, Option<bool>)> {
    let dir = adapters_dir(env).join(name);
    let settings = settings_path(spec, env, scope);
    let registered = settings_has_marker(&settings, spec.array_key, &format!("{}", dir.display()))?;
    let installed = crate::filesystem::regular_file(&dir.join(spec.primary))?.is_some();
    let expected: BTreeMap<String, String> = spec
        .files
        .iter()
        .map(|(n, b)| ((*n).to_string(), sha256_hex(b.as_bytes())))
        .collect();
    let (intact, current) = sidecar_integrity(&dir, &expected)?;
    Ok((installed, registered, intact, current))
}

fn status_plugin(
    spec: &PluginSpec,
    env: &AdapterEnv,
    scope: Scope,
) -> Result<(bool, bool, Option<bool>, Option<bool>)> {
    let dir = plugin_dir(spec, env, scope);
    let file = dir.join(spec.filename);
    let installed = crate::filesystem::regular_file(&file)?.is_some();
    let mut expected = BTreeMap::new();
    expected.insert(
        spec.filename.to_string(),
        sha256_hex(spec.source.as_bytes()),
    );
    let (intact, current) = sidecar_integrity(&dir, &expected)?;
    // A retained ownership record identifies a broken installation even when
    // its auto-discovered primary file has been removed.
    let registered = installed || intact.is_some();
    Ok((installed, registered, intact, current))
}

/// Two independent signals from the sidecar, against `dir` and this binary's
/// embedded artifacts. No sidecar → `(None, None)`.
///
/// - `intact` (tamper): `Some(true)` only when every recorded file is present
///   on disk and its on-disk sha256 matches the sidecar's recorded hash. A
///   missing file or a differing hash yields `Some(false)`.
/// - `current` (staleness): `Some(true)` when the sidecar's recorded hashes are
///   byte-for-byte what THIS binary embeds for the harness (`expected`); any
///   difference — a changed, added, or removed file — yields `Some(false)`.
fn sidecar_integrity(
    dir: &Path,
    expected: &BTreeMap<String, String>,
) -> Result<(Option<bool>, Option<bool>)> {
    let pending = crate::adapter::pending_sidecar_path(dir);
    if crate::filesystem::regular_file(&pending)?.is_some() {
        anyhow::bail!(
            "incomplete installation: pending adapter recovery at {}",
            pending.display()
        );
    }
    match read_sidecar(dir)? {
        Some(sc) => {
            let mut intact = true;
            for (name, recorded_hash) in &sc.files {
                if !expected.contains_key(name) {
                    intact = false;
                    continue;
                }
                let path = dir.join(name);
                match std::fs::read(&path) {
                    Ok(bytes) => intact &= sha256_hex(&bytes) == *recorded_hash,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => intact = false,
                    Err(error) => {
                        return Err(error).with_context(|| {
                            format!("reading adapter artifact {}", path.display())
                        })
                    }
                }
            }
            Ok((Some(intact), Some(is_current(&sc.files, expected))))
        }
        None => Ok((None, None)),
    }
}

/// The installed artifact is current when the hashes recorded at install time
/// exactly match what this binary embeds now. `BTreeMap` equality catches a
/// changed hash and an added/removed file alike.
fn is_current(recorded: &BTreeMap<String, String>, expected: &BTreeMap<String, String>) -> bool {
    recorded == expected
}

fn settings_has_marker(path: &Path, key: &str, marker: &str) -> Result<bool> {
    let value = load_settings(path)?;
    Ok(value
        .get("hooks")
        .and_then(|h| h.get(key))
        .map(|arr| {
            serde_json::to_string(arr)
                .unwrap_or_default()
                .contains(marker)
        })
        .unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{
        all_harnesses, plan_install, plan_uninstall, AdapterEnv, PlanStep, Scope,
    };

    fn harness(name: &str) -> crate::adapter::Harness {
        let mut harness = all_harnesses()
            .into_iter()
            .find(|h| h.name == name)
            .unwrap();
        // Most tests exercise the shared installer/uninstaller mechanics.
        // Cleanup-only legacy adapters remain constructible for those tests,
        // while the production registry keeps them non-installable.
        harness.installable = true;
        harness
    }

    #[test]
    fn cleanup_only_adapter_is_not_installed() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let harness = all_harnesses()
            .into_iter()
            .find(|h| h.name == "codex")
            .unwrap();
        let out = install(&harness, &e, Scope::Global).unwrap();
        assert!(matches!(out.result, InstallResult::Skipped(_)));
    }
    fn env(tmp: &std::path::Path) -> AdapterEnv {
        AdapterEnv {
            home: tmp.to_path_buf(),
            config_home: tmp.join(".config"),
            project_root: tmp.join("proj"),
        }
    }

    #[test]
    fn install_codex_writes_artifact_sidecar_and_patches_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let out = install(&harness("codex"), &e, Scope::Global).unwrap();
        assert!(matches!(out.result, InstallResult::Installed));
        let hook = e.config_home.join("ironlint/adapters/codex/hook.sh");
        assert!(hook.exists());
        assert!(crate::adapter::read_sidecar(hook.parent().unwrap())
            .unwrap()
            .is_some());
        let settings: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(tmp.path().join(".codex/hooks.json")).unwrap(),
        )
        .unwrap();
        let cmd = settings["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert!(cmd.contains("adapters/codex/hook.sh"));
        assert!(cmd.ends_with("pre-tool-use"));
    }

    #[test]
    fn install_claude_code_local_writes_settings_local_json_not_settings_json() {
        // Regression guard for the Finding: a Local-scope claude-code install
        // must never write to the committable `.claude/settings.json` — that
        // leaks the installing machine's absolute hook path (and $HOME) into
        // version control, and breaks teammates whose `hook.sh` lives
        // elsewhere. The hook entry belongs in the personal, gitignored
        // `.claude/settings.local.json` sidecar instead.
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let out = install(&harness("claude-code"), &e, Scope::Local).unwrap();
        assert!(matches!(out.result, InstallResult::Installed));

        let settings_local = e.project_root.join(".claude/settings.local.json");
        let settings_committed = e.project_root.join(".claude/settings.json");
        assert!(
            settings_local.exists(),
            "expected hook entry in {}",
            settings_local.display()
        );
        assert!(
            !settings_committed.exists(),
            "Local claude-code install must not create the committable {}",
            settings_committed.display()
        );

        let settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings_local).unwrap()).unwrap();
        let cmd = settings["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert!(cmd.contains("adapters/claude-code/hook.sh"));
        assert!(cmd.ends_with("pre-tool-use"));
    }

    #[test]
    fn install_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        install(&harness("codex"), &e, Scope::Global).unwrap();
        let again = install(&harness("codex"), &e, Scope::Global).unwrap();
        assert!(matches!(again.result, InstallResult::AlreadyPresent));
    }

    #[test]
    fn install_plugin_drops_file_in_project_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let out = install(&harness("opencode"), &e, Scope::Local).unwrap();
        assert!(matches!(out.result, InstallResult::Installed));
        assert!(e
            .project_root
            .join(".opencode/plugins/ironlint.ts")
            .exists());
    }

    #[test]
    fn uninstall_removes_artifact_and_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        install(&harness("codex"), &e, Scope::Global).unwrap();
        let out = uninstall(&harness("codex"), &e, Scope::Global).unwrap();
        assert!(matches!(out.result, InstallResult::Installed)); // "removed" reuses Installed-style ok
        assert!(!e
            .config_home
            .join("ironlint/adapters/codex/hook.sh")
            .exists());
        let settings: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(tmp.path().join(".codex/hooks.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(settings["hooks"]["PreToolUse"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn uninstall_jsonhook_preserves_user_edits_and_deregisters() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let h = harness("codex");
        install(&h, &e, Scope::Global).unwrap();
        let dir = e.config_home.join("ironlint/adapters/codex");
        let hook = dir.join("hook.sh");
        std::fs::write(&hook, b"#!/bin/sh\n# user edit\n").unwrap();
        let extra = dir.join("my-notes.txt");
        std::fs::write(&extra, b"keep me").unwrap();

        uninstall(&h, &e, Scope::Global).unwrap();
        let settings = std::fs::read_to_string(tmp.path().join(".codex/hooks.json")).unwrap();
        assert!(!settings.contains(&dir.display().to_string()));
        assert_eq!(std::fs::read(&hook).unwrap(), b"#!/bin/sh\n# user edit\n");
        assert_eq!(std::fs::read(&extra).unwrap(), b"keep me");
    }

    #[cfg(unix)]
    #[test]
    fn uninstall_jsonhook_does_not_follow_replaced_adapter_directory() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let h = harness("codex");
        install(&h, &e, Scope::Global).unwrap();
        let dir = e.config_home.join("ironlint/adapters/codex");
        let outside = tmp.path().join("outside");
        std::fs::rename(&dir, &outside).unwrap();
        symlink(&outside, &dir).unwrap();

        let out = uninstall(&h, &e, Scope::Global).unwrap();
        assert!(matches!(out.result, InstallResult::Skipped(_)));
        assert!(outside.join("hook.sh").exists());
        assert!(outside.join(".ironlint-adapter.json").exists());
        let settings = std::fs::read_to_string(tmp.path().join(".codex/hooks.json")).unwrap();
        assert!(!settings.contains(&dir.display().to_string()));
    }

    #[test]
    fn status_reports_installed_and_intact_after_install() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        std::fs::create_dir_all(tmp.path().join(".codex")).unwrap();
        install(&harness("codex"), &e, Scope::Global).unwrap();
        let st = status(&harness("codex"), &e, Scope::Global).unwrap();
        assert!(st.detected && st.installed && st.registered);
        assert_eq!(st.intact, Some(true));
        assert_eq!(st.current, Some(true));
    }

    #[test]
    fn status_current_false_when_recorded_hash_diverges_from_embedded() {
        // Staleness is derived by comparing the sidecar's recorded hashes to the
        // hashes THIS binary embeds. Simulate an artifact shipped by an older
        // binary: install fresh, then rewrite the sidecar so hook.sh's *recorded*
        // hash is bogus (≠ the embedded hash). `current` must flip to Some(false).
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let h = harness("codex");
        install(&h, &e, Scope::Global).unwrap();
        let dir = e.config_home.join("ironlint/adapters/codex");
        let mut sc = crate::adapter::read_sidecar(&dir).unwrap().unwrap();
        sc.files
            .insert("hook.sh".to_string(), "sha256:deadbeef".to_string());
        crate::adapter::write_sidecar(&dir, &sc).unwrap();
        let st = status(&h, &e, Scope::Global).unwrap();
        assert_eq!(
            st.current,
            Some(false),
            "recorded hash ≠ embedded hash must report current=false"
        );
    }

    #[test]
    fn is_current_true_only_on_exact_match() {
        let mut recorded = BTreeMap::new();
        recorded.insert("hook.sh".to_string(), "sha256:aa".to_string());
        // Equal maps → current.
        let expected = recorded.clone();
        assert!(is_current(&recorded, &expected));
        // Differing hash → not current.
        let mut changed = recorded.clone();
        changed.insert("hook.sh".to_string(), "sha256:bb".to_string());
        assert!(!is_current(&recorded, &changed));
        // Added key → not current.
        let mut added = recorded.clone();
        added.insert("extra.sh".to_string(), "sha256:cc".to_string());
        assert!(!is_current(&recorded, &added));
        // Removed key → not current.
        let empty = BTreeMap::new();
        assert!(!is_current(&recorded, &empty));
    }

    #[test]
    fn install_jsonhook_idempotent_leaves_settings_byte_identical() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        install(&harness("codex"), &e, Scope::Global).unwrap();
        let settings_path = tmp.path().join(".codex/hooks.json");
        let before = std::fs::read_to_string(&settings_path).unwrap();
        install(&harness("codex"), &e, Scope::Global).unwrap();
        let after = std::fs::read_to_string(&settings_path).unwrap();
        assert_eq!(
            before, after,
            "settings file must not be rewritten on AlreadyPresent"
        );
    }

    #[test]
    fn d2_settings_edits_reread_under_the_same_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let settings = e.home.join("shared.json");
        let guard = ResourceLocks::acquire(&[&settings]).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let child_barrier = barrier.clone();
        let child_env = e.clone();
        let thread = std::thread::spawn(move || {
            let mut h = harness("codex");
            let HarnessKind::JsonHook(spec) = &mut h.kind else {
                panic!("JSON hook")
            };
            spec.settings_global = |env| env.home.join("shared.json");
            child_barrier.wait();
            install(&h, &child_env, Scope::Global).unwrap();
        });
        barrier.wait();
        write_settings(&settings, &serde_json::json!({"firstIronLintEdit": true})).unwrap();
        drop(guard);
        thread.join().unwrap();
        let value = load_settings(&settings).unwrap();
        assert_eq!(value["firstIronLintEdit"], true);
        assert!(value["hooks"]["PreToolUse"].is_array());
    }

    #[test]
    fn install_plugin_identical_content_is_already_present() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        install(&harness("opencode"), &e, Scope::Local).unwrap();
        let again = install(&harness("opencode"), &e, Scope::Local).unwrap();
        assert!(
            matches!(again.result, InstallResult::AlreadyPresent),
            "identical re-install must return AlreadyPresent"
        );
    }

    #[test]
    fn d2_install_preserves_identical_foreign_plugin_without_adopting_it() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let h = harness("pi");
        let HarnessKind::Plugin(spec) = &h.kind else {
            panic!("Pi plugin")
        };
        let dir = plugin_dir(spec, &e, Scope::Local);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(spec.filename);
        std::fs::write(&file, spec.source).unwrap();
        let result = install(&h, &e, Scope::Local).unwrap();
        assert!(
            matches!(result.result, InstallResult::Skipped(_)),
            "foreign content must stay foreign"
        );
        assert!(read_sidecar(&dir).unwrap().is_none());
        assert_eq!(std::fs::read(&file).unwrap(), spec.source.as_bytes());
    }

    #[test]
    fn d2_reinstall_preserves_edited_owned_plugin() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let h = harness("pi");
        install(&h, &e, Scope::Local).unwrap();
        let file = e.project_root.join(".pi/extensions/ironlint.ts");
        std::fs::write(&file, b"// user edit").unwrap();
        let result = install(&h, &e, Scope::Local).unwrap();
        assert!(matches!(result.result, InstallResult::Skipped(_)));
        assert_eq!(std::fs::read(&file).unwrap(), b"// user edit");
    }

    #[test]
    fn install_plugin_changed_content_is_updated() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let mut h = harness("opencode");
        install(&h, &e, Scope::Local).unwrap();
        if let HarnessKind::Plugin(spec) = &mut h.kind {
            spec.source = "// upgraded";
        }
        let again = install(&h, &e, Scope::Local).unwrap();
        assert!(
            matches!(again.result, InstallResult::Updated),
            "changed content must return Updated"
        );
    }

    #[test]
    fn status_before_install_is_not_installed() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let st = status(&harness("codex"), &e, Scope::Global).unwrap();
        assert!(!st.installed);
        assert!(!st.registered);
        assert!(st.intact.is_none());
        assert!(st.current.is_none());
    }

    #[test]
    fn status_detects_on_disk_tamper() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let h = harness("codex");
        install(&h, &e, Scope::Global).unwrap();
        // Tamper with the installed artifact, leaving the sidecar untouched.
        let hook = e.config_home.join("ironlint/adapters/codex/hook.sh");
        let mut bytes = std::fs::read(&hook).unwrap();
        bytes.extend_from_slice(b"\n# tampered\n");
        std::fs::write(&hook, bytes).unwrap();
        let st = status(&h, &e, Scope::Global).unwrap();
        assert_eq!(
            st.intact,
            Some(false),
            "edited on-disk artifact must report intact=false"
        );
    }

    #[test]
    fn uninstall_plugin_removes_file() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        install(&harness("opencode"), &e, Scope::Local).unwrap();
        let file = e.project_root.join(".opencode/plugins/ironlint.ts");
        assert!(file.exists());
        uninstall(&harness("opencode"), &e, Scope::Local).unwrap();
        assert!(!file.exists(), "uninstall must remove the plugin file");
    }

    #[test]
    fn uninstall_plugin_preserves_edited_file() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let h = harness("opencode");
        install(&h, &e, Scope::Local).unwrap();
        let file = e.project_root.join(".opencode/plugins/ironlint.ts");
        std::fs::write(&file, b"// user edit").unwrap();
        let out = uninstall(&h, &e, Scope::Local).unwrap();
        assert!(matches!(out.result, InstallResult::Skipped(_)));
        assert_eq!(std::fs::read(&file).unwrap(), b"// user edit");
    }

    #[test]
    fn install_skill_writes_skill_md_and_sidecar() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let out = install_skill(&harness("pi"), &e, Scope::Local).unwrap();
        assert!(matches!(out.result, InstallResult::Installed));
        let skill = e.project_root.join(".pi/skills/ironlint-config/SKILL.md");
        assert!(skill.exists(), "SKILL.md must land at {}", skill.display());
        assert!(crate::adapter::read_sidecar(skill.parent().unwrap())
            .unwrap()
            .is_some());
    }

    #[test]
    fn install_skill_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        install_skill(&harness("pi"), &e, Scope::Local).unwrap();
        let again = install_skill(&harness("pi"), &e, Scope::Local).unwrap();
        assert!(matches!(again.result, InstallResult::AlreadyPresent));
    }

    #[test]
    fn install_skill_changed_content_is_updated() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let mut h = harness("pi");
        install_skill(&h, &e, Scope::Local).unwrap();
        h.skill.source = "// upgraded skill";
        let again = install_skill(&h, &e, Scope::Local).unwrap();
        assert!(matches!(again.result, InstallResult::Updated));
    }

    #[test]
    fn uninstall_skill_removes_the_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        install_skill(&harness("pi"), &e, Scope::Local).unwrap();
        let dir = e.project_root.join(".pi/skills/ironlint-config");
        assert!(dir.exists());
        uninstall_skill(&harness("pi"), &e, Scope::Local).unwrap();
        assert!(!dir.exists(), "uninstall must remove the skill dir");
    }

    #[test]
    fn uninstall_skill_preserves_edited_file() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let h = harness("pi");
        install_skill(&h, &e, Scope::Local).unwrap();
        let file = e.project_root.join(".pi/skills/ironlint-config/SKILL.md");
        std::fs::write(&file, b"# user edit").unwrap();
        let out = uninstall_skill(&h, &e, Scope::Local).unwrap();
        assert!(matches!(out.result, InstallResult::Skipped(_)));
        assert_eq!(std::fs::read(&file).unwrap(), b"# user edit");
    }

    #[test]
    fn install_skill_global_uses_home_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        install_skill(&harness("pi"), &e, Scope::Global).unwrap();
        // pi global skills dir is ~/.pi/agent/skills
        assert!(e
            .home
            .join(".pi/agent/skills/ironlint-config/SKILL.md")
            .exists());
    }

    #[test]
    fn plan_install_jsonhook_lists_files_patch_and_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let steps = plan_install(&harness("claude-code"), &e, Scope::Local);
        // one hook file + one patch + one skill
        let hooks = steps
            .iter()
            .filter(|s| matches!(s, PlanStep::Hook { .. }))
            .count();
        assert_eq!(hooks, 1, "claude-code ships hook.sh");
        assert!(steps
            .iter()
            .any(|s| matches!(s, PlanStep::Patch { key, .. } if *key == "PreToolUse")));
        assert!(steps.iter().any(|s| matches!(s, PlanStep::Skill { .. })));
    }

    #[test]
    fn plan_install_plugin_lists_plugin_and_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let steps = plan_install(&harness("pi"), &e, Scope::Local);
        assert!(steps.iter().any(|s| matches!(s, PlanStep::Plugin { .. })));
        assert!(steps.iter().any(|s| matches!(s, PlanStep::Skill { .. })));
        assert!(!steps.iter().any(|s| matches!(s, PlanStep::Patch { .. })));
    }

    #[test]
    fn plan_install_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let _ = plan_install(&harness("codex"), &e, Scope::Global);
        assert!(!e
            .config_home
            .join("ironlint/adapters/codex/hook.sh")
            .exists());
        assert!(!tmp.path().join(".codex/hooks.json").exists());
    }

    #[test]
    fn plan_uninstall_jsonhook_lists_dir_patch_and_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let steps = plan_uninstall(&harness("codex"), &e, Scope::Global);
        assert!(steps.iter().any(|s| matches!(s, PlanStep::Hook { .. })));
        assert!(steps.iter().any(|s| matches!(s, PlanStep::Patch { .. })));
        assert!(steps.iter().any(|s| matches!(s, PlanStep::Skill { .. })));
    }

    #[test]
    fn plan_uninstall_plugin_lists_file_and_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let steps = plan_uninstall(&harness("opencode"), &e, Scope::Local);
        assert!(steps.iter().any(|s| matches!(s, PlanStep::Plugin { .. })));
        assert!(steps.iter().any(|s| matches!(s, PlanStep::Skill { .. })));
    }
}
