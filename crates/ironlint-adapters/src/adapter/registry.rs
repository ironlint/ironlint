use crate::adapter::{AdapterEnv, Harness, HarnessKind, Source};
use serde_json::{json, Value};
use std::path::PathBuf;

// Cleanup-only harnesses retain inert bytes so the generic ownership machinery
// can still identify their former artifact names without shipping executable
// legacy behavior.
const CLEANUP_PLUGIN: &str = "// cleanup-only IronLint artifact\nexport {};\n";
const PI_PLUGIN: Source = Source::Package("pi/src/index.ts");
const IRONLINT_CONFIG_SKILL: Source = Source::Package("shared/ironlint-config/SKILL.md");

/// Skill name and install-dir leaf for the authoring skill.
pub const SKILL_NAME: &str = "ironlint-config";

#[derive(Clone, Copy)]
pub struct JsonHookSpec {
    pub settings_local: fn(&AdapterEnv) -> Option<PathBuf>,
    pub settings_global: fn(&AdapterEnv) -> PathBuf,
    pub array_key: &'static str,
    pub entry_arg: &'static str,
    pub primary: &'static str,
    pub files: &'static [(&'static str, Source)],
    pub manifest: Option<Source>,
    pub build_entry: fn(&str) -> Value,
}

#[derive(Clone, Copy)]
pub struct PluginSpec {
    pub dir_local: fn(&AdapterEnv) -> Option<PathBuf>,
    pub dir_global: fn(&AdapterEnv) -> Option<PathBuf>,
    pub filename: &'static str,
    pub source: Source,
    pub detect: fn(&AdapterEnv) -> bool,
}

/// Where a harness discovers `SKILL.md` files, and the shared skill bytes.
#[derive(Clone, Copy)]
pub struct SkillSpec {
    pub dir_local: fn(&AdapterEnv) -> PathBuf,
    pub dir_global: fn(&AdapterEnv) -> PathBuf,
    pub source: Source,
}

// --- per-harness entry builders (also unit-tested directly) ------------------
pub(crate) fn claude_build_entry(command: &str) -> Value {
    json!({"matcher": "Edit|Write|MultiEdit|NotebookEdit",
           "hooks": [{"type": "command", "command": command}]})
}

pub(crate) fn codex_build_entry(command: &str) -> Value {
    // 120s = 4x IronLint's default per-check wall-clock cap (30s,
    // `execution.timeout_secs` default). Checks run sequentially, so a file
    // matching several slow checks can take multiples of that cap; this
    // hook timeout must exceed the worst-case sequential-check budget or
    // Codex kills the hook process first and the edit lands ungated with no
    // signal to anyone. See docs/adapters/README.md "Timeout budget".
    //
    json!({"matcher": "apply_patch|Edit|Write",
           "hooks": [{"type": "command", "command": command,
                      "timeout": 120, "statusMessage": "ironlint check"}]})
}

// --- registry ----------------------------------------------------------------
const CLAUDE: JsonHookSpec = JsonHookSpec {
    settings_local: |e| Some(e.project_root.join(".claude").join("settings.local.json")),
    settings_global: |e| e.home.join(".claude").join("settings.json"),
    array_key: "PostToolUse",
    entry_arg: "post-tool-use",
    primary: "hook.sh",
    files: &[
        ("hook.sh", Source::Package("claude-code/hooks/hook.sh")),
        ("hook.py", Source::Package("shared/hooks/hook.py")),
        ("process.py", Source::Package("shared/hooks/process.py")),
        (
            "hooks.json",
            Source::Package("claude-code/hooks/hooks.json"),
        ),
    ],
    manifest: Some(Source::Package("claude-code/hooks/hooks.json")),
    build_entry: claude_build_entry,
};

const CODEX: JsonHookSpec = JsonHookSpec {
    settings_local: |e| Some(e.project_root.join(".codex").join("hooks.json")),
    settings_global: |e| e.home.join(".codex").join("hooks.json"),
    array_key: "PostToolUse",
    entry_arg: "post-tool-use",
    primary: "hook.sh",
    files: &[
        ("hook.sh", Source::Package("codex/hooks/hook.sh")),
        ("hook.py", Source::Package("shared/hooks/hook.py")),
        ("process.py", Source::Package("shared/hooks/process.py")),
        ("hooks.json", Source::Package("codex/hooks/hooks.json")),
    ],
    manifest: Some(Source::Package("codex/hooks/hooks.json")),
    build_entry: codex_build_entry,
};

const PI: PluginSpec = PluginSpec {
    dir_local: |e| Some(e.project_root.join(".pi").join("extensions")),
    dir_global: |e| Some(e.home.join(".pi").join("agent").join("extensions")),
    filename: "ironlint.ts",
    source: PI_PLUGIN,
    detect: |e| e.home.join(".pi").is_dir(),
};

const OPENCODE: PluginSpec = PluginSpec {
    dir_local: |e| Some(e.project_root.join(".opencode").join("plugins")),
    dir_global: |_| None, // opencode plugins are project-scoped (per adapter README)
    filename: "ironlint.ts",
    source: Source::Inline(CLEANUP_PLUGIN),
    detect: |e| {
        e.config_home.join("opencode").is_dir() || e.project_root.join(".opencode").is_dir()
    },
};

// --- per-harness skill specs -------------------------------------------------
const CLAUDE_SKILL: SkillSpec = SkillSpec {
    dir_local: |e| e.project_root.join(".claude").join("skills"),
    dir_global: |e| e.home.join(".claude").join("skills"),
    source: IRONLINT_CONFIG_SKILL,
};
const CODEX_SKILL: SkillSpec = SkillSpec {
    dir_local: |e| e.project_root.join(".codex").join("skills"),
    dir_global: |e| e.home.join(".codex").join("skills"),
    source: IRONLINT_CONFIG_SKILL,
};
const PI_SKILL: SkillSpec = SkillSpec {
    dir_local: |e| e.project_root.join(".pi").join("skills"),
    dir_global: |e| e.home.join(".pi").join("agent").join("skills"),
    source: IRONLINT_CONFIG_SKILL,
};
const OPENCODE_SKILL: SkillSpec = SkillSpec {
    dir_local: |e| e.project_root.join(".opencode").join("skills"),
    dir_global: |e| e.config_home.join("opencode").join("skills"),
    source: IRONLINT_CONFIG_SKILL,
};

pub fn all_harnesses() -> Vec<Harness> {
    vec![
        Harness {
            name: "claude-code",
            kind: HarnessKind::JsonHook(CLAUDE),
            restart_hint: "Reload Claude Code (or restart) — it picks up settings.json hooks.",
            skill: CLAUDE_SKILL,
            installable: true,
        },
        Harness {
            name: "codex",
            kind: HarnessKind::JsonHook(CODEX),
            restart_hint: "Restart Codex, then review+trust the ironlint hook when Codex prompts (non-managed hooks require trust).",
            skill: CODEX_SKILL,
            installable: true,
        },
        Harness {
            name: "pi",
            kind: HarnessKind::Plugin(PI),
            restart_hint: "Restart pi so it loads the new extension.",
            skill: PI_SKILL,
            installable: true,
        },
        Harness {
            name: "opencode",
            kind: HarnessKind::Plugin(OPENCODE),
            restart_hint: "Restart opencode so it loads the new plugin.",
            skill: OPENCODE_SKILL,
            installable: false,
        },
    ]
}

/// Whether `harness` looks installed on this machine.
///
/// Keyed on the harness **name**, not `array_key`: claude-code and codex
/// both register a `PostToolUse` hook (see `CLAUDE`/`CODEX` above), so a
/// dispatch on `array_key` alone can no longer distinguish them.
pub(crate) fn is_detected(harness: &Harness, env: &AdapterEnv) -> bool {
    let owned_artifacts = crate::adapter::adapters_dir(env)
        .join(harness.name)
        .exists();
    match &harness.kind {
        HarnessKind::JsonHook(_) => match harness.name {
            "claude-code" => {
                owned_artifacts
                    || env.home.join(".claude").is_dir()
                    || env.project_root.join(".claude").is_dir()
            }
            "codex" => {
                owned_artifacts
                    || env.home.join(".codex").is_dir()
                    || env.project_root.join(".codex").is_dir()
            }
            _ => false,
        },
        HarnessKind::Plugin(p) => owned_artifacts || (p.detect)(env),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{AdapterEnv, HarnessKind};
    use std::path::PathBuf;

    fn env_with(home: &str, project: &str) -> AdapterEnv {
        AdapterEnv {
            home: PathBuf::from(home),
            config_home: PathBuf::from(format!("{home}/.config")),
            project_root: PathBuf::from(project),
        }
    }

    #[test]
    fn four_harnesses_registered() {
        let names: Vec<_> = all_harnesses().iter().map(|h| h.name).collect();
        assert_eq!(names, vec!["claude-code", "codex", "pi", "opencode"]);
    }

    #[test]
    fn runtime_artifacts_are_nonempty() {
        for h in all_harnesses() {
            match &h.kind {
                HarnessKind::JsonHook(s) => {
                    assert!(!s.files.is_empty(), "{} has no files", h.name);
                    for (name, bytes) in s.files {
                        assert!(
                            !bytes.read().unwrap().is_empty(),
                            "{}/{} empty",
                            h.name,
                            name
                        );
                    }
                }
                HarnessKind::Plugin(p) => assert!(
                    !p.source.read().unwrap().is_empty(),
                    "{} plugin empty",
                    h.name
                ),
            }
        }
    }

    #[test]
    fn claude_entry_points_at_command_and_matcher() {
        let e = claude_build_entry("\"/x/hook.sh\" pre-tool-use");
        assert_eq!(e["matcher"], "Edit|Write|MultiEdit|NotebookEdit");
        assert_eq!(e["hooks"][0]["command"], "\"/x/hook.sh\" pre-tool-use");
    }

    // IronLint's default per-check wall-clock cap is 30s
    // (`config::types::default_timeout_secs`, pinned separately by
    // `execution_timeout_defaults_to_30` in config/types.rs). Checks run
    // sequentially, so a file matching several slow checks can burn
    // multiples of that cap before ironlint reports a verdict. Codex's own
    // hook `timeout` must clear that worst case, or Codex kills the hook
    // process first and the edit lands ungated — a silent bypass.
    const DEFAULT_PER_CHECK_CAP_SECS: u64 = 30;

    #[test]
    fn codex_entry_matches_apply_patch() {
        let e = codex_build_entry("\"/x/hook.sh\" pre-tool-use");
        assert_eq!(e["matcher"], "apply_patch|Edit|Write");
        assert_eq!(e["hooks"][0]["command"], "\"/x/hook.sh\" pre-tool-use");

        // Assert the *relationship*, not just the current literal, so a
        // future bump to either number can't quietly erode the headroom.
        let hook_timeout = e["hooks"][0]["timeout"].as_u64().expect("timeout is a u64");
        assert!(
            hook_timeout >= 4 * DEFAULT_PER_CHECK_CAP_SECS,
            "codex hook timeout ({hook_timeout}s) must be >= 4x the default \
             per-check cap ({DEFAULT_PER_CHECK_CAP_SECS}s) so several \
             sequential slow checks don't blow the harness's own hook budget"
        );
    }

    #[test]
    fn detect_reports_presence_per_home() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_str().unwrap();
        std::fs::create_dir_all(format!("{home}/.claude")).unwrap();
        std::fs::create_dir_all(format!("{home}/.pi")).unwrap();
        let env = env_with(home, home);
        let found: std::collections::BTreeMap<_, _> =
            crate::adapter::detect(&env).into_iter().collect();
        assert_eq!(
            found,
            std::collections::BTreeMap::from([
                ("claude-code", true),
                ("codex", false),
                ("opencode", false),
                ("pi", true),
            ])
        );
    }

    #[test]
    fn claude_and_codex_share_post_tool_use_but_detect_independently() {
        // Regression guard: claude-code and codex both register a
        // PostToolUse hook now, so `is_detected` must key off the harness
        // name, not `array_key` — otherwise claude-code would be (mis)detected
        // via ~/.codex, or vice versa.
        let harnesses = all_harnesses();
        let claude = harnesses.iter().find(|h| h.name == "claude-code").unwrap();
        let codex = harnesses.iter().find(|h| h.name == "codex").unwrap();
        match (&claude.kind, &codex.kind) {
            (HarnessKind::JsonHook(c), HarnessKind::JsonHook(r)) => {
                assert_eq!(c.array_key, "PostToolUse");
                assert_eq!(r.array_key, "PostToolUse");
            }
            _ => panic!("expected both to be JsonHook"),
        }
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_str().unwrap();
        std::fs::create_dir_all(format!("{home}/.codex")).unwrap();
        let env = env_with(home, home);
        assert!(
            !is_detected(claude, &env),
            "claude-code false-positive via ~/.codex"
        );
        assert!(is_detected(codex, &env));

        let tmp2 = tempfile::tempdir().unwrap();
        let home2 = tmp2.path().to_str().unwrap();
        std::fs::create_dir_all(format!("{home2}/.claude")).unwrap();
        let env2 = env_with(home2, home2);
        assert!(is_detected(claude, &env2));
        assert!(
            !is_detected(codex, &env2),
            "codex false-positive via ~/.claude"
        );
    }

    #[test]
    fn skill_dirs_resolve_per_harness() {
        let env = env_with("/home/u", "/home/u/proj");
        let by = |name: &str| {
            all_harnesses()
                .into_iter()
                .find(|harness| harness.name == name)
                .unwrap()
                .skill
        };
        // claude-code
        let claude = by("claude-code");
        assert_eq!(
            (claude.dir_local)(&env),
            PathBuf::from("/home/u/proj/.claude/skills")
        );
        assert_eq!(
            (claude.dir_global)(&env),
            PathBuf::from("/home/u/.claude/skills")
        );
        // pi
        let pi = by("pi");
        assert_eq!(
            (pi.dir_local)(&env),
            PathBuf::from("/home/u/proj/.pi/skills")
        );
        assert_eq!(
            (pi.dir_global)(&env),
            PathBuf::from("/home/u/.pi/agent/skills")
        );
        // opencode (global lives under config_home)
        let opencode = by("opencode");
        assert_eq!(
            (opencode.dir_local)(&env),
            PathBuf::from("/home/u/proj/.opencode/skills")
        );
        assert_eq!(
            (opencode.dir_global)(&env),
            PathBuf::from("/home/u/.config/opencode/skills")
        );
        // codex
        let codex = by("codex");
        assert_eq!(
            (codex.dir_local)(&env),
            PathBuf::from("/home/u/proj/.codex/skills")
        );
        assert_eq!(
            (codex.dir_global)(&env),
            PathBuf::from("/home/u/.codex/skills")
        );
    }

    #[test]
    fn every_harness_ships_the_same_skill_source() {
        for h in all_harnesses() {
            assert!(
                String::from_utf8(h.skill.source.read().unwrap())
                    .unwrap()
                    .contains("name: ironlint-config"),
                "{} skill source wrong",
                h.name
            );
        }
    }

    #[test]
    fn opencode_remains_cleanup_only() {
        let h = all_harnesses()
            .into_iter()
            .find(|h| h.name == "opencode")
            .unwrap();
        assert!(!h.installable);
        let HarnessKind::Plugin(spec) = h.kind else {
            panic!("plugin expected")
        };
        assert_eq!(spec.source.read().unwrap(), CLEANUP_PLUGIN.as_bytes());
    }
}
