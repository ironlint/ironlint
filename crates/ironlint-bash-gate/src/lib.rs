//! The bash-gate matcher: a pure classifier for a Bash command string.
//!
//! Decides whether a command an agent wants to run would let it free itself
//! from ironlint's gate — `ironlint trust`, a Bash write to the policy
//! surface (`.ironlint.yml`, `.ironlint/scripts/`), a Bash write to the
//! adapter installation surface (the harness settings files / plugin dirs
//! `init` installs into — W3 harness self-defense), the W1 git pre-commit
//! floor hook, or a git-floor bypass form (`--no-verify`, `core.hooksPath`).
//! Pure of I/O and state except the process HOME (used to normalize `~` /
//! `$HOME` in adapter-surface paths — see `decide_with_home`); the
//! `ironlint gate-bash` subcommand and the adapter hooks are thin shims
//! around it. See `docs/superpowers/specs/2026-07-06-bash-gate-self-trust-prevention-design.md`.
//!
//! Threat tier: lazy non-reasoning models. Blocks direct forms + light
//! de-obfuscation, including git's `GIT_CONFIG_*` env-injection spelling of
//! `core.hooksPath` (see [`is_git_config_env_escape`]). Variable-substitution
//! indirection is a documented known gap (catching it needs real shell
//! evaluation — adversarial tier, out of scope). The test module pins both
//! directions.
//!
//! Residual (W3-R4): home-scoped harness settings (`~/.claude/settings.json`)
//! sit outside every repo glob's reach, so the agent's *file tools* can edit
//! them ungated — this module closes only the Bash path. Project-scoped
//! settings ARE repo paths; a user who wants file-tool coverage adds a normal
//! check scoping them (the guide suggests it). The git-bypass forms (W3-R3)
//! share the var-substitution gap and only affect agent Bash tool calls — a
//! human typing at a terminal is untouched.

#![warn(clippy::cognitive_complexity)]

/// The bash-gate's verdict on one command string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Allow the command to proceed.
    Allow,
    /// Block it; the string is the reason shown to the agent.
    Block(String),
}

/// Decide whether `command` may run.
///
/// Pure: no I/O, no state. Returns `Block(reason)` for `ironlint trust` (any
/// args) and Bash writes to the policy surface; `Allow` otherwise, including
/// the documented indirection gap (which is *intentionally* allowed).
/// The reason prefix used for every block. Adapters show the full reason to
/// the agent; keeping a stable prefix makes the contract tests robust to
/// wording changes.
const TRUST_REASON: &str = "ironlint trust must be run by a human, not by an agent";

/// Normalize a command for matching: collapse the light de-obfuscation cases
/// (backtick/`$()` delimiters around tokens, quoted binary names, runs of
/// whitespace) without attempting to evaluate the string. This is string
/// surgery, not a shell parser — variable-substitution indirection
/// (`iron$(echo lint)`) is deliberately untouched (known gap).
fn normalize(command: &str) -> String {
    // 1. Strip backtick and $() DELIMITERS while keeping their contents, so
    //    `ironlint` and $(ironlint) collapse to ironlint. Only the matched
    //    pair delimiters are removed; a stray backtick or unmatched paren is
    //    left alone (it does not denote a completed substitution).
    //
    //    Iterator-driven (no manual index arithmetic) so there is no `+=` to
    //    mutate into a hang; the `$(` opener consumes its `(` via peek+next.
    let mut s = String::with_capacity(command.len());
    let mut chars = command.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // backtick delimiter — drop (both opening and closing are skipped).
            '`' => continue,
            // `$(` opener — drop the `$` here and the `(` via next(). A `$`
            // NOT followed by `(` is a var sigil (`$FOO`) and is preserved by
            // falling through to the catch-all. The fall-through (not a guard)
            // keeps a `$` with no following `(` observable: `$ironlint trust`
            // must NOT collapse to `ironlint trust` (it's a var ref, not the
            // binary) — though as a var ref with no value it's a no-op for our
            // purposes; the important behavior is that only a true `$(` opener
            // strips the `(`.
            '$' => {
                if chars.peek() == Some(&'(') {
                    let _ = chars.next();
                    continue;
                }
                s.push('$');
            }
            // Shell grouping chars — `(`, `)`, `{`, `}`. Dropping them turns
            // `(ironlint trust)` and `{ ironlint trust; }` into `ironlint trust`
            // (the `;` is a separator handled by the caller's segment split).
            // These denote grouping, not tokens; dropping them is safe for
            // matching and does not collapse indirection (a `$(` is already
            // consumed above before its `(` could reach here).
            '(' | ')' | '{' | '}' => continue,
            other => s.push(other),
        }
    }

    // 2. Strip single/double quotes around the leading binary token, so
    //    'ironlint' trust and "ironlint" trust collapse to ironlint trust.
    //    Only the quote chars are removed; quoted strings otherwise survive.
    s = s.chars().filter(|&c| c != '\'' && c != '"').collect();

    // 3. Trim leading/trailing whitespace. (Runs of internal whitespace are NOT
    //    collapsed here — every consumer re-splits on `split_whitespace()`,
    //    which collapses internally, so a collapse step would be dead code that
    //    only generates un-killable mutation survivors. `strip_wrappers` and
    //    `is_policy_write` both `split_whitespace`, so `ironlint   trust` and
    //    `ironlint\ttrust` normalize to the same token list regardless.)
    s.trim().to_string()
}

/// Split a normalized command into independently-checkable segments at the
/// shell command separators `&&`, `||`, `;`, and `|`. A `trust` (or a policy
/// write) in ANY segment blocks — `ironlint check || ironlint trust` is the
/// textbook lazy escape. String surgery on the separators, not shell
/// evaluation; a pipe inside a quoted string would be mis-split, but quotes
/// are stripped in `normalize` and the threat tier is lazy models, not
/// adversarial quoting.
fn segments(normalized: &str) -> Vec<String> {
    // `>|` (clobber redirect) is the only redirect operator containing `|`.
    // Protect it with a NUL sentinel before splitting on `|` (pipe), then
    // restore it in each segment. The two-char separators `&&`/`||` are
    // collapsed to a DISTINCT sentinel (`;`-equivalent) so a lone `|` of a
    // split `||` isn't double-counted. Both sentinels are NUL-free bytes a
    // Bash command can't contain.
    const CLOBBER: &str = "\u{0}";
    const SEP: &str = "\u{1}";
    let protected = normalized.replace(">|", CLOBBER);
    let with_seps = protected.replace("&&", SEP).replace("||", SEP);
    with_seps
        .split([';', '|', '\u{1}'])
        .map(str::trim)
        .filter(|seg| !seg.is_empty())
        .map(|seg| seg.replace(CLOBBER, ">|").to_string())
        .collect()
}

/// The command prefixes that wrap an ironlint invocation without changing
/// its meaning: `nohup`, `env [VAR=val]...`, `exec`, `eval`, `timeout <N>`,
/// and `sh`/`bash -c '<cmd>'`. A lazy model prepends these to "make sure it
/// runs"; stripping them recovers the direct form. Bounded explicit list —
/// not shell evaluation.
fn strip_wrappers(segment: &str) -> String {
    // Drop leading wrapper prefixes (nohup/env/exec/eval/timeout, plus
    // sh/bash -c descent) one at a time. Uses `split_first` over a slice
    // cursor — no `+=`/`+` index arithmetic to mutate-hang or mutate-survive.
    let all: Vec<&str> = segment.split_whitespace().collect();
    let mut rest = all.as_slice();
    while let Some((first, tail)) = rest.split_first() {
        match *first {
            "nohup" | "exec" | "eval" => rest = tail,
            // env itself + any leading VAR=val assignments it carries.
            "env" => rest = skip_assignments(tail),
            // timeout + its single duration arg.
            "timeout" => rest = tail.split_first().map(|(_, t)| t).unwrap_or(&[]),
            // `sh -c '<cmd>'` / `bash -c "<cmd>"` (and `dash`/`ash`/`zsh`/`ksh`,
            // the same `-c` command-string shape): the command string is the
            // argument after `-c`. normalize() already stripped the quotes, so
            // its tokens are the slice beyond `-c` — descend into it and let
            // the loop re-check (mirrors eval/exec unwrapping to its argument).
            // `dash` is /bin/sh on Debian/Ubuntu; `ash` is the BusyBox sh
            // (Alpine/containers); a lazy model that knows its shell emits the
            // specific name. Without `-c` (`sh script.sh`), the shell runs a
            // script file — the documented indirection gap (adversarial tier);
            // don't descend.
            "sh" | "bash" | "dash" | "ash" | "zsh" | "ksh" if tail.first() == Some(&"-c") => {
                rest = &tail[1..]
            }
            // Bare `VAR=val ironlint trust`: a leading assignment with no
            // `env` wrapper is semantically identical to `env VAR=val ...`
            // (sh exports the assignment to the command's env). Skip it one
            // token at a time so the loop re-checks the next token — multiple
            // leading assignments (`FOO=bar BAZ=qux ironlint trust`) all get
            // stripped, then `ironlint` is the first non-assignment token and
            // the loop breaks, returning `ironlint trust` for is_ironlint_trust.
            // Strict identifier check (valid shell name before `=`) avoids
            // over-skipping a malformed leading flag like `--config=x.yml`.
            t if is_assignment(t) => rest = tail,
            _ => break,
        }
    }
    rest.join(" ")
}

/// Given the tokens AFTER `env`, drop `env`'s leading `VAR=val` assignments
/// and return the slice that follows them (the actual command). Iterator-
/// driven so a mutation to the skip logic can't hang.
fn skip_assignments<'a>(tail: &'a [&'a str]) -> &'a [&'a str] {
    let end = tail.iter().take_while(|t| is_assignment(t)).count();
    &tail[end..]
}

/// True if `token` is a shell `VAR=val` assignment: a `=` not at position 0,
/// with the pre-`=` part a valid shell identifier (letters/digits/underscore,
/// not starting with a digit). Shared by `skip_assignments` (post-`env`) and
/// `strip_wrappers`'s bare-prefix arm. The strict identifier check avoids
/// over-skipping a leading `--config=x.yml` (which contains `=` but is a
/// flag, not an assignment).
fn is_assignment(token: &str) -> bool {
    let Some(eq) = token.find('=') else {
        return false;
    };
    if eq == 0 {
        return false;
    }
    let name = &token[..eq];
    let mut chars = name.chars();
    let first = chars.next();
    // Shell identifier: first char a letter or underscore (not a digit); the
    // rest letters/digits/underscore. `IRONLINT_ROOT`, `FOO`, `_x9` match;
    // `9x`, `--config`, `a-b` don't.
    let ok_first = first.is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    ok_first && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// True if a token is the ironlint binary: the literal name, or a path ending
/// in `/ironlint` (absolute) or `./ironlint` (relative). Indirection
/// (`$IRON`) is NOT matched — that's the documented known gap.
fn is_ironlint_binary(token: &str) -> bool {
    token == "ironlint" || token.ends_with("/ironlint") || token == "./ironlint"
}

/// Given a tokenized segment with the ironlint binary at `bin_idx`, decide
/// whether `trust` follows as the subcommand. Handles global flags before the
/// subcommand: `ironlint --config x.yml trust`, `ironlint -v trust`. The rule
/// is permissive in the flag tokens but strict on the subcommand: if ANY
/// non-flag token after the binary is a read-only subcommand (`check`,
/// `doctor`), it's not a trust invocation. Otherwise, if `trust` appears as a
/// token after the binary, block.
///
/// `ironlint check --config x` (read-only subcommand THEN a flag) must allow —
/// the read-only subcommand short-circuits. `ironlint --config x.yml trust`
/// blocks (flag, then trust). `ironlint --config x.yml check` allows (flag,
/// then check). `ironlint trust --config x` blocks (trust first).
/// True if `trust` follows the ironlint binary at `tokens[bin_idx]`, after
/// skipping any global flags. The first non-flag token decides: `trust` blocks;
/// a read-only subcommand allows. Iterator-driven (no index arithmetic to
/// mutate-hang or mutate-survive).
fn trust_after_binary(tokens: &[&str], bin_idx: usize) -> bool {
    for t in &tokens[bin_idx + 1..] {
        if *t == "trust" {
            return true;
        }
        if !is_flag_token(t) {
            // A read-only subcommand (or any non-flag non-trust token) — allow.
            return false;
        }
        // A flag — skip and keep scanning.
    }
    false
}

/// True if `token` looks like a flag: a `--long[=val]` or a `-x` short flag.
/// Used to skip global flags between the ironlint binary and its subcommand.
fn is_flag_token(token: &str) -> bool {
    token.starts_with("--") || (token.starts_with('-') && token.len() == 2)
}

/// True if the segment invokes `ironlint trust` — the direct form plus the
/// light de-obfuscation a lazy model reaches for: path-prefixed binary names
/// (`/usr/local/bin/ironlint`, `./ironlint`), wrapper prefixes (`nohup`,
/// `env`, `exec`, `eval`, `timeout`), and global flags before the subcommand
/// (`ironlint --config x.yml trust`). Does NOT match read-only subcommands
/// (`check`, `doctor`, etc.).
///
/// Checks EVERY ironlint binary occurrence in the segment, not just the first
/// token — `ironlint check or ironlint trust` has a second binary (`or` is not
/// a shell operator, so segments() leaves it as one segment) whose `trust`
/// subcommand the first-binary scan misses (it short-circuits on `check`).
/// The first-token-is-binary guard still holds, so `echo ... ironlint trust`
/// (a string argument to echo) stays a non-match.
fn is_ironlint_trust(segment: &str) -> bool {
    let stripped = strip_wrappers(segment);
    let tokens: Vec<&str> = stripped.split_whitespace().collect();
    // The ironlint binary must be the FIRST token of the (wrapper-stripped)
    // segment — `echo ... ironlint trust` is a string argument to echo, not a
    // trust invocation. This is the false-positive guard: `echo "run
    // ironlint trust to bless"` starts with `echo`, not `ironlint`.
    let Some(&first) = tokens.first() else {
        return false;
    };
    if !is_ironlint_binary(first) {
        return false;
    }
    // A second ironlint binary buried in the segment (e.g. after a bare `or`)
    // is a separate invocation whose `trust` subcommand the first-binary scan
    // misses — check each occurrence. `trust_after_binary` itself stays strict:
    // `ironlint check trust` (one binary, `trust` as a stray positional to
    // `check`) still allows, because clap rejects the positional and nothing
    // fires; only a real second `ironlint trust` invocation blocks.
    tokens
        .iter()
        .enumerate()
        .any(|(idx, t)| is_ironlint_binary(t) && trust_after_binary(&tokens, idx))
}

/// `cd .ironlint && trust` and `cd .ironlint/scripts && trust`: a bare `trust`
/// in a later segment after a `cd` into the policy dir (`.ironlint` itself
/// or anything under `.ironlint/`). A bare `trust` elsewhere is not a trust
/// invocation. The `cd` target must start the `.ironlint` path component (not
/// `cd .ironlintfoo`). Returns true if some segment cds into the policy dir
/// and a later segment is exactly `trust` (with optional trailing args).
fn cd_into_policy_then_trust(segs: &[String]) -> bool {
    // Find the FIRST segment that cds into the policy dir, then check whether
    // a LATER segment runs `trust`. Scoping the trust check to segments after
    // the cd (not `any` over all segments) makes the bare-`trust` equality
    // observable: under a `== -> !=` mutation, the cd segment itself would
    // satisfy `!= "trust"` and false-block.
    //
    // The `cd_idx + 1` is intrinsic to mutate-kill: the cd segment (`cd
    // .ironlint`) can never itself be `trust`, so including it (`+ -> *` =
    // `cd_idx * 1`) yields the same verdict. Decomposing to avoid the `+ 1`
    // would reintroduce the `==` survivor this structure was built to kill.
    let cd_idx = segs.iter().position(|s| {
        s.strip_prefix("cd .ironlint")
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    });
    let Some(cd_idx) = cd_idx else { return false };
    segs[cd_idx + 1..]
        .iter()
        .any(|s| s == "trust" || s.starts_with("trust "))
}

/// True if a path token refers to the ironlint policy surface: the literal
/// `.ironlint.yml` (at any depth — bare or path-prefixed) or anything under
/// `.ironlint/scripts/`. Matched on the path string, not the filesystem.
fn is_policy_path(token: &str) -> bool {
    // `.ironlint.yml` as a SUFFIX (covers the bare token and any path-prefixed
    // form like `./.ironlint.yml` or `sub/.ironlint.yml`), OR `.ironlint.yml`
    // appearing with a leading slash mid-token (covered by suffix already, so
    // this arm is only reached when suffix is false — kept distinct so a
    // mutation flipping one operator is observable), OR a scripts dir prefix.
    //
    // The arms are intentionally non-redundant for the test corpus: the bare
    // `.ironlint.yml` exercises only the suffix arm; `x/.ironlint.yml.bak`
    // (suffix `.bak`, contains `/.ironlint.yml`) exercises only the contains
    // arm; `path/.ironlint.yml` exercises suffix (and would also hit contains,
    // but suffix short-circuits). Don't fold them.
    token.ends_with(".ironlint.yml")
        || token.contains("/.ironlint.yml")
        || token.contains(".ironlint/scripts/")
}

// ---------------------------------------------------------------------------
// W3 — harness self-defense: the adapter installation surface + the git floor
// ---------------------------------------------------------------------------
//
// gate-bash already blocks Bash writes to the ironlint policy surface. W3
// extends the same write-op family to the adapter installation artifacts —
// the settings files and plugin dirs `ironlint init` writes — plus the W1
// git pre-commit floor hook. An agent that can `rm ~/.claude/settings.json`
// or `.git/hooks/pre-commit` through Bash can uninstall its own rail; these
// paths are protected the same way the policy files are.
//
// The sets below mirror `crates/ironlint-core/src/adapter/registry.rs`
// (`settings_*` / `dir_*` of every adapter). The parity test in ironlint-cli
// (W3-R2) keeps the two in lockstep — add a fifth adapter there and the gate
// stops compiling before it silently under-covers it. Skills dirs are NOT in
// the set: they are skill-install targets, not harness hooks (a removed
// skill degrades guidance, it does not remove the rail).

/// Repo-relative / home-relative forms of the harness **installation
/// artifacts**. Files match as a path SUFFIX, which covers the bare
/// (`.claude/settings.json`), project-relative (`sub/.claude/settings.json`),
/// and absolute (`/Users/x/.claude/settings.json`) spellings in one arm.
pub const ADAPTER_SURFACE_FILES: &[&str] = &[
    ".claude/settings.json",
    ".claude/settings.local.json",
    ".codex/hooks.json",
];

/// Directory install targets; any write UNDER one of these is blocked.
/// Project-scoped `.pi/extensions` and `.opencode/plugins` (the registry has
/// no opencode global), home-scoped `~/.pi/agent/extensions`, plus the
/// PARENT dirs of the settings files (`.claude`, `.codex`) so `rm -rf
/// ~/.claude` can't delete the rails sideways. Dirs match as full path
/// components (see [`is_surface_dir`]) so a `.pi/extensions-backup` lookalike
/// stays writable. The parity test keeps this equal to the registry-derived
/// set (`adapter_install_surface`, which emits the same parent dirs).
pub const ADAPTER_SURFACE_DIRS: &[&str] = &[
    ".claude",
    ".codex",
    ".pi/extensions",
    ".pi/agent/extensions",
    ".opencode/plugins",
];

/// The EXECUTABLE subset of [`ADAPTER_SURFACE_DIRS`]: writing a plugin or
/// hook under one of these installs a rail-bypass executable — the exact W3
/// self-trust gap (round-2 over-block-1 reconciliation). These gate ALL write
/// families. The remaining dirs (`.claude`, `.codex` — broad profile dirs
/// holding skills/agents next to the protected settings FILES) gate only the
/// DELETION family, so Bash-authored skill files under them stay legal while
/// `rm -rf ~/.claude` still blocks.
pub const ADAPTER_SURFACE_EXEC_DIRS: &[&str] = &[
    ".pi/extensions",
    ".pi/agent/extensions",
    ".opencode/plugins",
];

/// True if a path token refers to an adapter installation FILE — the exact
/// `settings.json` / `hooks.json` documents. These gate ALL write families;
/// `~`, `$HOME`, and absolute spellings normalize against the process HOME;
/// bare project-relative forms match without it. This is the one place the
/// otherwise-pure classifier reads the environment (the process env is a
/// legitimate input: gate-bash runs as a spawned process).
fn is_adapter_surface_file(token: &str, home: Option<&str>) -> bool {
    let expanded = expand_home(token, home);
    ADAPTER_SURFACE_FILES.iter().any(|f| expanded.ends_with(f))
}

/// True if a path token falls under an EXECUTABLE adapter surface dir
/// (`.pi/extensions`, `.pi/agent/extensions`, `.opencode/plugins`). These
/// gate ALL write families (round-2 over-block-1 reconciliation — the W3
/// self-trust surface is executable, not skill-authoring).
fn is_adapter_surface_exec_dir(token: &str, home: Option<&str>) -> bool {
    let expanded = expand_home(token, home);
    ADAPTER_SURFACE_EXEC_DIRS
        .iter()
        .any(|d| is_surface_dir(&expanded, d))
}

/// True if a path token falls under a PROFILE adapter surface dir — the
/// broad dirs (`.claude`, `.codex`) whose protected settings FILES sit
/// inside. These gate ONLY the DELETION family: Bash-authoring a skill file
/// under `.claude/` is a legitimate edit, but `rm -rf`/`chmod` of the whole
/// dir (or a file in it) deletes the rail (round-2 over-block 1). Derived as
/// `ADAPTER_SURFACE_DIRS` minus the exec subset so new registry dirs default
/// to deletion-only and the W3-R2 parity test still covers the union.
fn is_adapter_surface_profile_dir(token: &str, home: Option<&str>) -> bool {
    let expanded = expand_home(token, home);
    ADAPTER_SURFACE_DIRS
        .iter()
        .filter(|d| !ADAPTER_SURFACE_EXEC_DIRS.contains(d))
        .any(|d| is_surface_dir(&expanded, d))
}

/// `~`/`$HOME` normalization: a leading `~`, or the FIRST `~/` / `$HOME/`
/// occurrence (a token like `>~/.claude/settings.json` has the tilder
/// mid-token after the redirect op — normalize() has already stripped
/// quotes). `home=None` leaves the token as-is: bare project-relative forms
/// still match project-scoped surfaces.
fn expand_home(token: &str, home: Option<&str>) -> String {
    let Some(h) = home else {
        return token.to_string();
    };
    if let Some(rest) = token.strip_prefix('~') {
        return format!("{h}{rest}");
    }
    for needle in ["~/", "$HOME/"] {
        if let Some(i) = token.find(needle) {
            let mut out = String::with_capacity(token.len() + h.len());
            out.push_str(&token[..i]);
            out.push_str(h);
            out.push('/');
            out.push_str(&token[i + needle.len()..]);
            return out;
        }
    }
    token.to_string()
}

/// Directory-form matching: the token IS `dir`, sits under it, or carries it
/// as a full path component (absolute spellings end with `/{dir}`).
/// `.pi/extensions-backup` must NOT match — no `/` boundary after the dir.
fn is_surface_dir(token: &str, dir: &str) -> bool {
    token == dir
        || token.starts_with(&format!("{dir}/"))
        || token.ends_with(&format!("/{dir}"))
        || token.contains(&format!("/{dir}/"))
}

/// The W1 git pre-commit floor hook is protected the same way: an agent must
/// not remove, chmod, or overwrite its own commit-boundary rail (W3-R3).
/// Suffix match covers the bare `.git/hooks/pre-commit` and any absolute
/// spelling; linked-worktree hooks live under `.git/worktrees/*/hooks/` in
/// the repo, still `.git/...`-prefixed.
fn is_floor_hook_path(token: &str) -> bool {
    token.ends_with(".git/hooks/pre-commit")
        || (token.ends_with('*') && token.contains(".git/hooks/pre-commit"))
}

/// True if a path token refers to any protected FILE: the ironlint policy
/// files, the adapter installation files, or the git floor hook. These exact
/// files gate ALL write families (redirect, tee, cp/mv dest, editors,
/// deletion).
fn is_protected_path(token: &str, home: Option<&str>) -> bool {
    is_policy_path(token)
        || is_adapter_surface_file(token, home)
        || is_floor_hook_path(token)
        || is_adapter_surface_exec_dir(token, home)
}

/// The DELETION family (rm/rmdir/chmod/chown/unlink/truncate) and `mv`-source
/// additionally protect whole DIRECTORY surfaces — including `.git/hooks`, the
/// floor hook's parent (`rm -rf .git/hooks` removes the rail without ever
/// naming pre-commit, round-2 finding 4). Content-write families (`>`, cp
/// dest, editors) deliberately exclude dirs: authoring a file under a surface
/// dir via Bash stays legal (round-2 over-block 1).
fn is_delete_target_protected(token: &str, home: Option<&str>) -> bool {
    is_protected_path(token, home)
        || is_adapter_surface_profile_dir(token, home)
        || is_surface_dir(&expand_home(token, home), ".git/hooks")
}

/// W3-R3 git-floor bypass forms: `git commit --no-verify`, `git -c
/// core.hooksPath=… commit`, `git config core.hooksPath …` — the lazy-agent
/// escapes from the commit-boundary rail (with W1 shipped, `--no-verify` IS
/// the obvious escape). Only fires on agent Bash tool calls; a human typing
/// at a terminal is untouched. Shares the documented var-substitution gap.
fn is_git_commit_escape(segment: &str) -> bool {
    // Wrapper prefixes (`env`, leading VAR=val, `nohup`, `timeout <N>`,
    // `sh -c`) must not shelter the bypass (2026-08-20 review): strip them
    // before tokenizing, mirroring the trust detector.
    let stripped = strip_wrappers(segment);
    let tokens: Vec<&str> = stripped.split_whitespace().collect();
    if tokens.first() != Some(&"git") {
        return false;
    }
    let sub = git_subcommand(&tokens);
    for (i, t) in tokens.iter().enumerate() {
        match *t {
            // Long form covers EVERY hook-running subcommand.
            "--no-verify" => return true,
            // Commit's short verify is a single-dash bundle containing `n`
            // (`-n`, `-qn`). ONLY commit: for merge/pull `-n` is `--no-stat`,
            // for cherry-pick/revert `--no-commit` (round-2 finding 5). The
            // bundle scan stops n-detection after value-taking letters so
            // `-cuser.name=x` never false-blocks (round-2 finding 3).
            t if sub == Some("commit")
                && t.starts_with('-')
                && !t.starts_with("--")
                && t.len() > 1
                && short_bundle_has_verify(t) =>
            {
                return true;
            }
            // `-c core.hooksPath=…` config injection (mutating -- it re-routes
            // hooks for the invocation).
            "-c" if tokens
                .get(i + 1)
                .is_some_and(|v| v.to_lowercase().contains("core.hookspath")) =>
            {
                return true;
            }
            // Glued `-ccore.hooksPath=…` (keys are case-insensitive).
            t if t.starts_with("-c")
                && t.len() > 2
                && t.to_lowercase().contains("core.hookspath") =>
            {
                return true;
            }
            "config" if config_mutates_hooks_path(&tokens) => return true,
            _ => {}
        }
    }
    false
}

/// Git's `GIT_CONFIG_*` env vars are the documented-feature equivalent of the
/// `-c <key>=<value>` spellings already blocked in [`is_git_commit_escape`]:
/// `GIT_CONFIG_COUNT=n` plus `GIT_CONFIG_KEY_i`/`GIT_CONFIG_VALUE_i` inject
/// config pairs, and `GIT_CONFIG_PARAMETERS` carries a pre-encoded `-c` list.
/// `env GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/tmp/x git commit -m x`
/// re-routes the commit floor with no visible `-c`/`config` token, so match
/// the KEY-CARRYING var directly: block when the assignment token names
/// `core.hooksPath` (case-insensitive).
///
/// Scoped to the assignment token, so the COUNT/VALUE companions and an
/// innocent `GIT_CONFIG_GLOBAL=<file>` stay allowed — the former carry only a
/// count/path value, the latter redirects the global config FILE, not the
/// hooks path. Mirrors [`config_mutates_hooks_path`]'s read/write split: no
/// `core.hooksPath` inside the injecting var, no re-route.
fn is_git_config_env_escape(segment: &str) -> bool {
    // The env spelling only injects when the segment actually invokes git AND
    // the assignment precedes the git binary (2026-08-20 review): after it,
    // the token is an argument (a config VALUE, an echo payload) — not
    // environment.
    if strip_wrappers(segment).split_whitespace().next() != Some("git") {
        return false;
    }
    segment
        .split_whitespace()
        .take_while(|t| *t != "git")
        .any(|t| {
            let lower = t.to_lowercase();
            (lower.starts_with("git_config_key_") || lower.starts_with("git_config_parameters"))
                && lower.contains("core.hookspath")
        })
}

/// True if a single-dash short-flag bundle contains `-n` (git commit's
/// `--no-verify`) before any value-taking flag, whose value would otherwise be
/// scanned for `n`. `-qn` = `-q -n` matches; `-cuser.name=chris` and `-m msg`
/// do NOT.
fn short_bundle_has_verify(bundle: &str) -> bool {
    for ch in bundle[1..].chars() {
        match ch {
            'n' => return true,
            'c' | 'C' | 'm' | 'M' | 'F' | 'o' | 'O' => return false,
            _ => {}
        }
    }
    false
}

/// True if the `git config` invocation MUTATES `core.hooksPath` (a
/// case-insensitive key). Reads — `--get`, `--get-all`, or a bare
/// `config core.hooksPath` with no value — stay allowed (round-2 over-block
/// 2); only value assignment and the mutating sub-options block.
fn config_mutates_hooks_path(tokens: &[&str]) -> bool {
    let Some(ci) = tokens.iter().position(|t| *t == "config") else {
        return false;
    };
    let after = &tokens[ci + 1..];
    let mutating_flag = after.iter().any(|t| {
        matches!(
            *t,
            "--add" | "--unset" | "--unset-all" | "--rename-section" | "--remove-section"
        )
    });
    if mutating_flag {
        // Mutating flags block only when the key IS core.hooksPath —
        // `--unset user.name` is not a floor re-route (2026-08-20 review).
        return after.iter().any(|t| t.to_lowercase() == "core.hookspath");
    }
    // A bare key followed by a non-flag value is a plain set (`git config k v`).
    for (i, t) in after.iter().enumerate() {
        if t.to_lowercase() == "core.hookspath"
            && after.get(i + 1).is_some_and(|n| !n.starts_with('-'))
        {
            return true;
        }
    }
    false
}

/// The first non-flag token AFTER the `git` binary — the subcommand. Skips
/// value-taking adjustments (`-c`/`-C`/`--config`) AND their values, so a
/// `-c key=val` before the subcommand resolves correctly
/// (`git -c user.name=x commit` -> `commit`, round-2 finding 1).
fn git_subcommand<'a>(tokens: &[&'a str]) -> Option<&'a str> {
    let mut i = 1;
    while i < tokens.len() {
        let t = tokens[i];
        if matches!(t, "-c" | "-C" | "--config") {
            i += 2; // skip the flag and its value
            continue;
        }
        if t.starts_with('-') {
            i += 1; // other flag, no value
            continue;
        }
        return Some(t);
    }
    None
}

/// `rm`/`chmod`/`chown`/`rmdir` targeting a protected path — deletion and
/// permission changes are the most direct way to uninstall a rail (the
/// redirect/tee/editor family only covers content writes). Any non-flag
/// argument that names a protected path blocks; `chmod 755 file` and
/// `chmod +x file` skip their mode operand naturally (it is not a path).
fn rm_chmod_targets_protected(tokens: &[&str], home: Option<&str>) -> bool {
    let Some(&cmd) = tokens.first() else {
        return false;
    };
    matches!(
        cmd,
        "rm" | "chmod" | "chown" | "rmdir" | "unlink" | "truncate"
    ) && tokens
        .iter()
        .skip(1)
        .any(|t| !t.starts_with('-') && is_delete_target_protected(t, home))
}

/// True if the normalized segment writes to a protected surface. Detected via:
///   - redirect operators targeting a protected path: >, >>, >|, &>, &>>
///     (bare, start-glued, or end-glued to the preceding arg)
///   - `tee` writing a protected path
///   - in-place editors: `sed -i`, `ed`, `perl -i`
///   - `cp`/`mv`/`install`/`rsync` with a protected path as the DESTINATION
///   - `dd of=<protected path>` / `sponge <protected path>`
///   - `rm`/`chmod`/`chown`/`rmdir` naming a protected path
///
/// A protected path as a SOURCE (e.g. `cp ~/.claude/settings.json /tmp/backup`,
/// `dd if=.ironlint.yml of=/tmp/backup`) is a read and MUST allow — only the
/// destination is checked.
fn is_protected_write(segment: &str, home: Option<&str>) -> bool {
    let tokens: Vec<&str> = segment.split_whitespace().collect();

    redirect_targets_protected(&tokens, home)
        || tee_targets_protected(&tokens, home)
        || inplace_editor_targets_protected(&tokens, home)
        || cp_mv_destination_is_protected(&tokens, home)
        || dd_targets_protected(&tokens, home)
        || sponge_targets_protected(&tokens, home)
        || rm_chmod_targets_protected(&tokens, home)
}

/// The redirect operators we gate, longest-first so `>>` is tried before `>`.
const REDIRECT_OPS: &[&str] = &["&>>", ">>", "&>", ">|", ">"];

/// Redirect operators targeting a protected path. Three forms:
///   - bare operator + next token: `> .ironlint.yml`
///   - operator glued to the START of a token: `>.ironlint.yml`
///   - operator glued to the END of a preceding arg: `echo x>.ironlint.yml`
///     (the most common form a model emits — no space before the `>`).
fn redirect_targets_protected(tokens: &[&str], home: Option<&str>) -> bool {
    for (i, t) in tokens.iter().enumerate() {
        // Bare operator: `> .ironlint.yml` — the NEXT token is the target.
        if REDIRECT_OPS.contains(t) {
            if let Some(next) = tokens.get(i + 1) {
                if is_protected_path(next, home) {
                    return true;
                }
            }
        }
        // Glued (start OR end): a token that contains a redirect op AND ends
        // with a protected path. `>.ironlint.yml` and `x>.ironlint.yml` both
        // satisfy `ends_with(".ironlint.yml")` and contain a redirect op, so one
        // check covers both — no need to split the token. Skip the bare-op
        // tokens (handled above) to avoid a double-count false signal.
        if !REDIRECT_OPS.contains(t) && contains_redirect_op(t) && is_protected_path(t, home) {
            return true;
        }
    }
    false
}

/// True if `token` contains any redirect operator (`>`, `>>`, `>|`, `&>`,
/// `&>>`) anywhere in it. Used to distinguish a glued redirect
/// (`x>.ironlint.yml`) from a plain path token (`.ironlint.yml`).
fn contains_redirect_op(token: &str) -> bool {
    REDIRECT_OPS.iter().any(|op| token.contains(op))
}

/// `tee` / `tee -a`: a later argument is the destination file. `tee` may
/// appear after a pipe (`echo x | tee .ironlint.yml`), so scan for it as any
/// token, then check the non-flag arguments that follow it.
fn tee_targets_protected(tokens: &[&str], home: Option<&str>) -> bool {
    let mut seen_tee = false;
    for t in tokens {
        if seen_tee && !t.starts_with('-') && is_protected_path(t, home) {
            return true;
        }
        if *t == "tee" {
            seen_tee = true;
        }
    }
    false
}

/// In-place editors: `sed -i ... <file>`, `ed -s <file>`, `perl -i ... <file>`.
/// The last non-flag argument is the target file. Only block if `-i` is
/// present (sed/perl) or it's `ed` (which edits in place by nature).
fn inplace_editor_targets_protected(tokens: &[&str], home: Option<&str>) -> bool {
    let Some(&cmd) = tokens.first() else {
        return false;
    };
    let inplace = match cmd {
        "ed" => true,
        "sed" | "perl" => {
            // `-i` exactly (bare) OR an `-i`-prefixed flag (`-i.bak`). Both
            // forms denote in-place editing; either is sufficient. The two
            // checks are intentionally separate so a mutation to one alone
            // is caught by the form that exercises only the other.
            has_bare_dash_i(tokens) || has_dash_i_prefixed_flag(tokens)
        }
        _ => false,
    };
    inplace
        && tokens
            .last()
            .is_some_and(|last| is_protected_path(last, home))
}

/// True if any token is exactly `-i` (the bare in-place flag).
fn has_bare_dash_i(tokens: &[&str]) -> bool {
    tokens.contains(&"-i")
}

/// True if any token starts with `-i` but is longer (e.g. `-i.bak`).
fn has_dash_i_prefixed_flag(tokens: &[&str]) -> bool {
    tokens.iter().any(|t| t.starts_with("-i") && *t != "-i")
}

/// `cp`/`mv`/`install`/`rsync` with a protected path as the DESTINATION. The
/// destination is the last argument (for both two-arg and multi-source forms).
/// A protected path as a SOURCE (e.g. `cp ~/.claude/settings.json /tmp/backup`)
/// MUST allow — that's why only the last token is checked. `install` and
/// `rsync` share the cp/mv destination semantics for our purposes.
fn cp_mv_destination_is_protected(tokens: &[&str], home: Option<&str>) -> bool {
    let Some(&cmd) = tokens.first() else {
        return false;
    };
    let dest_protected = tokens
        .last()
        .is_some_and(|dest| is_protected_path(dest, home));
    // `mv` MOVES the rail away: a protected SOURCE is as destructive as a
    // protected destination. `cp`/`install`/`rsync` read the source, so for
    // them only the destination matters.
    // `mv` MOVES the rail away: a protected SOURCE is as destructive as a
    // protected destination. Matches files + exec dirs (not profile dirs:
    // relocating a skill file out of `.claude/` is benign).
    let mv_source_protected = cmd == "mv"
        && tokens
            .get(1..tokens.len().saturating_sub(1))
            .is_some_and(|srcs| srcs.iter().any(|s| is_protected_path(s, home)));
    matches!(cmd, "cp" | "mv" | "install" | "rsync")
        && tokens.len() >= 3
        && (dest_protected || mv_source_protected)
}

/// `dd of=<protected path>`: dd writes via its `of=` operand, not a positional
/// arg. Block if any token is `of=<protected path>` OR `of` followed by a
/// protected path token. `dd if=.ironlint.yml of=/tmp/backup` (protected as
/// INPUT) MUST allow — only the `of=` destination is checked.
fn dd_targets_protected(tokens: &[&str], home: Option<&str>) -> bool {
    let is_dd = tokens.first().is_some_and(|c| *c == "dd");
    if !is_dd {
        return false;
    }
    for (i, t) in tokens.iter().enumerate() {
        // Glued: `of=.ironlint.yml`.
        if let Some(rest) = t.strip_prefix("of=") {
            if is_protected_path(rest, home) {
                return true;
            }
        }
        // Separated: `of .ironlint.yml`.
        if *t == "of" {
            if let Some(next) = tokens.get(i + 1) {
                if is_protected_path(next, home) {
                    return true;
                }
            }
        }
    }
    false
}

/// `sponge <file>` (from moreutils): writes its stdin to a file. The file is
/// the LAST argument. `echo x | sponge .ironlint.yml` is a protected write.
fn sponge_targets_protected(tokens: &[&str], home: Option<&str>) -> bool {
    let is_sponge = tokens.first().is_some_and(|c| *c == "sponge");
    is_sponge
        && tokens
            .last()
            .is_some_and(|last| is_protected_path(last, home))
}

/// Decided block reason for a protected-surface write.
const PROTECTED_WRITE_REASON: &str = "ironlint policy files must be edited through the Write/Edit tool (which is gated), not via Bash — harness/plugin files and the git hook are protected the same way";

/// Decided block reason for a git-floor bypass form.
const GIT_ESCAPE_REASON: &str = "git commit bypass (--no-verify / core.hooksPath) and .git/hooks/pre-commit writes are blocked: they would remove the ironlint floor";

/// Decide whether `command` may run.
///
/// Pure except for the process HOME (used to normalize `~`/`$HOME` in
/// adapter-surface paths — see [`decide_with_home`]). Returns
/// `Block(reason)` for `ironlint trust` (any args, in any command segment),
/// Bash writes to the protected surface (policy files, adapter installation
/// artifacts, the git floor hook), and git-floor bypass forms; `Allow`
/// otherwise, including the documented indirection gap (which is
/// *intentionally* allowed). A `trust` or protected write in ANY segment of a
/// chained command (`a && ironlint trust`, `check || trust`) blocks — the
/// whole command is denied.
pub fn decide(command: &str) -> Decision {
    decide_with_home(command, std::env::var("HOME").ok().as_deref())
}

/// [`decide`] with an explicit HOME, so tests can normalize `~`/`$HOME`
/// against a controlled path instead of the process environment.
pub fn decide_with_home(command: &str, home: Option<&str>) -> Decision {
    let n = normalize(command);
    let segs = segments(&n);

    // `cd .ironlint && trust`: a bare `trust` after a `cd` into the policy
    // dir. The `&&` splits this into two segments, so check across the
    // segment list — does any segment cd into the policy dir, and does a
    // later segment run a bare `trust`?
    if cd_into_policy_then_trust(&segs) {
        return Decision::Block(TRUST_REASON.to_string());
    }
    for seg in &segs {
        if is_ironlint_trust(seg) {
            return Decision::Block(TRUST_REASON.to_string());
        }
        if is_protected_write(seg, home) {
            return Decision::Block(PROTECTED_WRITE_REASON.to_string());
        }
        if is_git_commit_escape(seg) {
            return Decision::Block(GIT_ESCAPE_REASON.to_string());
        }
        if is_git_config_env_escape(seg) {
            return Decision::Block(GIT_ESCAPE_REASON.to_string());
        }
    }
    Decision::Allow
}

#[cfg(test)]
mod tests {
    use super::{decide, decide_with_home, Decision};

    /// Assert `decide(cmd)` blocks; the reason is checked loosely (caller
    /// cares that it blocked, not the exact wording).
    fn assert_blocks(cmd: &str) {
        match decide(cmd) {
            Decision::Block(_) => {}
            Decision::Allow => panic!("expected Block for {cmd:?}, got Allow"),
        }
    }

    /// Assert `decide(cmd)` allows.
    fn assert_allows(cmd: &str) {
        match decide(cmd) {
            Decision::Allow => {}
            Decision::Block(r) => panic!("expected Allow for {cmd:?}, got Block({r:?})"),
        }
    }

    // --- (1) ironlint trust, any args ---
    #[test]
    fn blocks_bare_ironlint_trust() {
        assert_blocks("ironlint trust");
    }

    #[test]
    fn blocks_ironlint_trust_with_config_flag() {
        assert_blocks("ironlint trust --config shared/base.yml");
    }

    #[test]
    fn blocks_ironlint_trust_with_dot_arg() {
        assert_blocks("ironlint trust .");
    }

    // --- light de-obfuscation: backtick / $() around the binary name ---
    #[test]
    fn blocks_backtick_ironlint_trust() {
        assert_blocks("`ironlint` trust");
    }

    #[test]
    fn blocks_dollar_paren_ironlint_trust() {
        assert_blocks("$(ironlint) trust");
    }

    // --- light de-obfuscation: quoted binary name ---
    #[test]
    fn blocks_single_quoted_ironlint_trust() {
        assert_blocks("'ironlint' trust");
    }

    #[test]
    fn blocks_double_quoted_ironlint_trust() {
        assert_blocks("\"ironlint\" trust");
    }

    // --- light de-obfuscation: whitespace around the binary token ---
    #[test]
    fn blocks_ironlint_trust_extra_spaces() {
        assert_blocks("ironlint   trust");
    }

    #[test]
    fn blocks_ironlint_trust_tab_separated() {
        assert_blocks("ironlint\ttrust");
    }

    // --- cd .ironlint && trust (bare trust after cd into policy dir) ---
    #[test]
    fn blocks_cd_ironlint_then_trust() {
        assert_blocks("cd .ironlint && trust");
    }

    #[test]
    fn blocks_cd_ironlint_scripts_then_trust() {
        assert_blocks("cd .ironlint/scripts && trust");
    }

    // --- false-positive guard: 'cd .ironlintfoo' is NOT the policy dir ---
    // The boundary check after `cd .ironlint` exists to reject lookalike
    // dirs (`.ironlintfoo`, `.ironlint-backup`). Without a test pinning the
    // rejection, a mutation flipping the boundary predicate survives.
    #[test]
    fn allows_cd_ironlintfoo_lookalike() {
        assert_allows("cd .ironlintfoo && trust");
    }

    #[test]
    fn allows_cd_ironlint_backup_lookalike() {
        assert_allows("cd .ironlint-backup && trust");
    }

    // --- false-positive guard: read-only ironlint subcommands MUST allow ---
    #[test]
    fn allows_ironlint_check() {
        assert_allows("ironlint check");
    }

    #[test]
    fn allows_ironlint_doctor() {
        assert_allows("ironlint doctor");
    }

    #[test]
    fn allows_ironlint_validate() {
        assert_allows("ironlint validate");
    }

    #[test]
    fn allows_ironlint_explain() {
        assert_allows("ironlint explain");
    }

    #[test]
    fn allows_ironlint_show_resolved_config() {
        assert_allows("ironlint show-resolved-config");
    }

    #[test]
    fn allows_ironlint_init() {
        assert_allows("ironlint init");
    }

    // --- false-positive guard: 'ironlint trust' as a STRING, not a command ---
    #[test]
    fn allows_echo_quoting_ironlint_trust() {
        assert_allows("echo \"run ironlint trust to bless\"");
    }

    // --- (2) Bash writes to the policy surface: redirects ---
    #[test]
    fn blocks_redirect_to_ironlint_yml() {
        assert_blocks("echo x > .ironlint.yml");
    }

    #[test]
    fn blocks_append_to_ironlint_yml() {
        assert_blocks("echo x >> .ironlint.yml");
    }

    #[test]
    fn blocks_clobber_redirect_to_ironlint_yml() {
        assert_blocks("echo x >| .ironlint.yml");
    }

    #[test]
    fn blocks_amp_redirect_to_ironlint_yml() {
        assert_blocks("echo x &> .ironlint.yml");
    }

    #[test]
    fn blocks_amp_append_to_ironlint_yml() {
        assert_blocks("echo x &>> .ironlint.yml");
    }

    #[test]
    fn blocks_cat_redirect_to_ironlint_yml() {
        assert_blocks("cat > .ironlint.yml");
    }

    // --- tee ---
    #[test]
    fn blocks_tee_ironlint_yml() {
        assert_blocks("echo x | tee .ironlint.yml");
    }

    #[test]
    fn blocks_tee_append_ironlint_yml() {
        assert_blocks("echo x | tee -a .ironlint.yml");
    }

    // `tee` as the FIRST token (no pipe) — still a write to the policy path.
    // Pins the `idx + 1` slice boundary: a `tee` at index 0 must scan its own
    // following args, not the token before it.
    #[test]
    fn blocks_tee_as_first_token_ironlint_yml() {
        assert_blocks("tee .ironlint.yml");
    }

    // --- in-place editors ---
    #[test]
    fn blocks_sed_inplace_ironlint_yml() {
        assert_blocks("sed -i 's/x/y/' .ironlint.yml");
    }

    // `sed -iEXT` (in-place with a backup extension) is the same write as
    // `sed -i`. Pinning it closes a mutation gap: the `-i` detector must
    // match the `-i.bak` form, not just the bare `-i` token.
    #[test]
    fn blocks_sed_inplace_with_backup_ext_ironlint_yml() {
        assert_blocks("sed -i.bak 's/x/y/' .ironlint.yml");
    }

    #[test]
    fn blocks_perl_inplace_ironlint_yml() {
        assert_blocks("perl -i -pe 's/x/y/' .ironlint.yml");
    }

    // `perl -iEXT` (in-place with backup extension) — same pin as sed.
    #[test]
    fn blocks_perl_inplace_with_backup_ext_ironlint_yml() {
        assert_blocks("perl -i.bak -pe 's/x/y/' .ironlint.yml");
    }

    #[test]
    fn blocks_ed_ironlint_yml() {
        assert_blocks("ed -s .ironlint.yml");
    }

    // `sed` (or `perl`) editing a policy file WITHOUT any `-i` flag is a READ
    // (sed streams to stdout), so it MUST allow. This single test pins three
    // mutants at once: if `has_bare_dash_i` or `has_dash_i_prefixed_flag`
    // mutates to always-true, or the `&&` in the prefixed check flips to `||`,
    // this command flips from Allow to Block.
    #[test]
    fn allows_sed_without_inplace_flag_on_policy_file() {
        assert_allows("sed 's/x/y/' .ironlint.yml");
    }

    #[test]
    fn allows_perl_without_inplace_flag_on_policy_file() {
        assert_allows("perl -pe 's/x/y/' .ironlint.yml");
    }

    // --- same detectors against .ironlint/scripts/ ---
    #[test]
    fn blocks_redirect_to_policy_script() {
        assert_blocks("echo x > .ironlint/scripts/lint.sh");
    }

    #[test]
    fn blocks_sed_inplace_policy_script() {
        assert_blocks("sed -i 's/x/y/' .ironlint/scripts/lint.sh");
    }

    // `.ironlint.yml` appearing mid-token but NOT as a suffix (the file is
    // something else with `.ironlint.yml` embedded behind a slash, e.g. a
    // backup `sub/.ironlint.yml.bak`). Pins `is_policy_path`'s
    // `contains("/.ironlint.yml")` arm independently of the suffix arm — a
    // mutation flipping that operator to `&&` lets this through.
    #[test]
    fn blocks_redirect_to_embedded_ironlint_yml_non_suffix() {
        assert_blocks("echo x > sub/.ironlint.yml.bak");
    }

    // --- cp / mv ONTO a policy path (destination) ---
    #[test]
    fn blocks_cp_onto_policy_script() {
        assert_blocks("cp malicious.sh .ironlint/scripts/lint.sh");
    }

    #[test]
    fn blocks_mv_onto_ironlint_yml() {
        assert_blocks("mv bad.yml .ironlint.yml");
    }

    // --- false-positive guard: policy path as SOURCE (a read), not destination ---
    #[test]
    fn allows_cp_from_ironlint_yml_as_source() {
        assert_allows("cp .ironlint.yml /tmp/backup");
    }

    #[test]
    fn allows_cat_ironlint_yml_piped_to_grep() {
        assert_allows("cat .ironlint.yml | grep checks");
    }

    #[test]
    fn allows_grep_recursive_ironlint() {
        assert_allows("grep -r ironlint docs/");
    }

    #[test]
    fn allows_ls_ironlint_scripts() {
        assert_allows("ls .ironlint/scripts/");
    }

    #[test]
    fn allows_cat_policy_script() {
        assert_allows("cat .ironlint/scripts/lint.sh");
    }

    // `tee` to a NON-policy file must allow — pins the `&&` (not `||`) in
    // tee_targets_policy's non-flag check, so a benign `tee /tmp/log` is
    // never false-blocked.
    #[test]
    fn allows_tee_to_non_policy_file() {
        assert_allows("echo x | tee /tmp/log");
    }

    // --- every Bash tool call reaches decide(); pin ordinary commands
    //     allow through the gate (no more pre-filter skipping) ---
    #[test]
    fn allows_cargo_test() {
        assert_allows("cargo test");
    }

    #[test]
    fn allows_git_status() {
        assert_allows("git status");
    }

    // --- documented known gap: variable-substitution indirection MUST allow ---
    #[test]
    fn allows_iron_echo_lint_indirection() {
        assert_allows("iron$(echo lint) trust");
    }

    #[test]
    fn allows_ironvar_trust_indirection() {
        assert_allows("IRON=ironlint; $IRON trust");
    }

    #[test]
    fn allows_base64_eval_indirection() {
        assert_allows("base64 -d <<< 'aXJvbmxpbnQgdHJ1c3Q=' | sh");
    }

    #[test]
    fn allows_bash_script_indirection() {
        assert_allows("bash scripts/x.sh");
    }

    // --- normalize edge: a lone '$' not followed by '(' must not be treated
    //     as a $() opener (which would skip 2 chars and could mask a token).
    //     `echo $` is a benign command that must allow; pinning it closes a
    //     mutation gap on the `c == '$' && i+1 < len && chars[i+1] == '('`
    //     guard in normalize(). ---
    #[test]
    fn allows_echo_trailing_dollar() {
        assert_allows("echo $");
    }

    // =====================================================================
    // Code-review hardening (2026-07-06): direct-form bypasses a lazy
    // non-reasoning model plausibly emits. These are NOT indirection — the
    // binary is invoked by literal name / the write targets a literal path.
    // =====================================================================

    // --- F1: end-glued redirect (op glued to the PRECEDING arg's tail) ---
    // `echo x>.ironlint.yml` — the MOST common form a model emits (no space
    // before the `>`). All redirect variants must block here too.
    #[test]
    fn blocks_end_glued_redirect_to_ironlint_yml() {
        assert_blocks("echo x>.ironlint.yml");
    }

    #[test]
    fn blocks_end_glued_append_to_ironlint_yml() {
        assert_blocks("echo x>>.ironlint.yml");
    }

    #[test]
    fn blocks_end_glued_clobber_to_ironlint_yml() {
        assert_blocks("echo x>|.ironlint.yml");
    }

    #[test]
    fn blocks_end_glued_amp_redirect_to_ironlint_yml() {
        assert_blocks("echo x&>.ironlint.yml");
    }

    #[test]
    fn blocks_end_glued_amp_append_to_ironlint_yml() {
        assert_blocks("echo x&>>.ironlint.yml");
    }

    // --- F2: chained commands — `ironlint trust` after a separator ---
    // `ironlint check || ironlint trust` is the textbook lazy escape: "the
    // check failed, so just trust it." Each separator form must block.
    #[test]
    fn blocks_chained_ampamp_ironlint_trust() {
        assert_blocks("echo done && ironlint trust");
    }

    #[test]
    fn blocks_chained_oror_ironlint_trust() {
        assert_blocks("ironlint check || ironlint trust");
    }

    #[test]
    fn blocks_chained_semicolon_ironlint_trust() {
        assert_blocks("echo hi; ironlint trust");
    }

    #[test]
    fn blocks_chained_pipe_ironlint_trust() {
        assert_blocks("echo hi | ironlint trust");
    }

    // A chained trust with args (defense against `... && ironlint trust -c x`).
    #[test]
    fn blocks_chained_ironlint_trust_with_args() {
        assert_blocks("true && ironlint trust --config x.yml");
    }

    // `or` is NOT a shell operator (that's `||`), so a lazy model confusing
    // the two writes `ironlint check or ironlint trust`. sh runs `ironlint
    // check`, then `or` (command not found), then `ironlint trust` — trust
    // fires. segments() doesn't split on bare `or` (correctly — it's not a
    // separator), so the whole string is one segment; the fix is to catch
    // `trust` as ANY token after the ironlint binary in a segment, not just
    // the first non-flag one. Safe because no subcommand legitimately takes
    // `trust` as an argument (`explain` takes a file path).
    #[test]
    fn blocks_ironlint_check_or_ironlint_trust() {
        assert_blocks("ironlint check or ironlint trust");
    }

    // --- F3: prefix wrappers (`nohup`, `env`, `exec`, `eval`, `timeout`) ---
    #[test]
    fn blocks_nohup_ironlint_trust() {
        assert_blocks("nohup ironlint trust");
    }

    #[test]
    fn blocks_env_ironlint_trust() {
        assert_blocks("env ironlint trust");
    }

    #[test]
    fn blocks_exec_ironlint_trust() {
        assert_blocks("exec ironlint trust");
    }

    #[test]
    fn blocks_eval_ironlint_trust() {
        assert_blocks("eval ironlint trust");
    }

    #[test]
    fn blocks_timeout_ironlint_trust() {
        assert_blocks("timeout 5 ironlint trust");
    }

    // `env VAR=val ironlint trust` (env with an assignment) — a wrapper with
    // a leading var assignment still wraps a direct invocation.
    #[test]
    fn blocks_env_with_assignment_ironlint_trust() {
        assert_blocks("env IRONLINT_TIMEOUT=10 ironlint trust");
    }

    // --- F4: full-path / relative-path invocation ---
    #[test]
    fn blocks_absolute_path_ironlint_trust() {
        assert_blocks("/usr/local/bin/ironlint trust");
    }

    #[test]
    fn blocks_cargo_bin_path_ironlint_trust() {
        assert_blocks("/Users/me/.cargo/bin/ironlint trust");
    }

    #[test]
    fn blocks_relative_path_ironlint_trust() {
        assert_blocks("./ironlint trust");
    }

    // --- F5: global flags before the `trust` subcommand ---
    // `ironlint -v trust` and `ironlint --verbose trust` (no-value global
    // flags) must block. NOTE: `ironlint --config x.yml trust` is rejected by
    // clap at runtime (`--config` is a per-subcommand flag, not global), so a
    // model emitting it can't actually run trust that way — but the no-value
    // global-flag forms are real and must block.
    #[test]
    fn blocks_global_verbose_flag_before_trust() {
        assert_blocks("ironlint -v trust");
    }

    #[test]
    fn blocks_global_long_verbose_flag_before_trust() {
        assert_blocks("ironlint --verbose trust");
    }

    #[test]
    fn blocks_global_quiet_flag_before_trust() {
        assert_blocks("ironlint -q trust");
    }

    // --- F6: additional write primitives (dd, install, rsync, sponge) ---
    #[test]
    fn blocks_dd_of_ironlint_yml() {
        assert_blocks("dd if=/dev/zero of=.ironlint.yml bs=1 count=1");
    }

    #[test]
    fn blocks_dd_of_policy_script() {
        assert_blocks("dd of=.ironlint/scripts/x.sh");
    }

    // `dd of .ironlint.yml` (SEPARATED form — `of` then the path) pins the
    // `tokens.get(i + 1)` next-token lookup. Under a `+ -> -` mutation the
    // check would read the token BEFORE `of` (not the path) and miss it.
    #[test]
    fn blocks_dd_separated_of_ironlint_yml() {
        assert_blocks("dd if=/dev/zero of .ironlint.yml");
    }

    #[test]
    fn blocks_install_onto_ironlint_yml() {
        assert_blocks("install -m 644 bad.yml .ironlint.yml");
    }

    #[test]
    fn blocks_rsync_onto_ironlint_yml() {
        assert_blocks("rsync bad.yml .ironlint.yml");
    }

    #[test]
    fn blocks_sponge_ironlint_yml() {
        assert_blocks("echo x | sponge .ironlint.yml");
    }

    // --- F?: subshell / brace-group grouping around a direct trust ---
    #[test]
    fn blocks_subshell_ironlint_trust() {
        assert_blocks("(ironlint trust)");
    }

    #[test]
    fn blocks_brace_group_ironlint_trust() {
        assert_blocks("{ ironlint trust; }");
    }

    // --- regression guards: the new detectors must NOT over-block reads ---
    // `ironlint --config x.yml check` is a legit read (flag before a read-only
    // subcommand) and must allow — pins that the global-flag-skip only fires
    // for `trust`, not every subcommand.
    #[test]
    fn allows_global_config_flag_before_check() {
        assert_allows("ironlint --config x.yml check");
    }

    // `nohup ironlint check` is a legit read wrapped in nohup — allow.
    #[test]
    fn allows_nohup_ironlint_check() {
        assert_allows("nohup ironlint check");
    }

    // `env ironlint doctor` — allow (read-only subcommand, even wrapped).
    #[test]
    fn allows_env_ironlint_doctor() {
        assert_allows("env ironlint doctor");
    }

    // A chained `ironlint check` after a separator must allow (it's a read).
    #[test]
    fn allows_chained_ironlint_check() {
        assert_allows("echo done && ironlint check");
    }

    // `ironlint xy trust` (a 2-char NON-flag token before `trust`) must ALLOW —
    // `xy` is not a flag, so the first non-flag token short-circuits. Pins
    // `is_flag_token`'s `&&`: under `||`, a 2-char token is mis-flagged, skipped,
    // and `trust` is reached → false block.
    #[test]
    fn allows_two_char_nonflag_before_trust() {
        assert_allows("ironlint xy trust");
    }

    // `ironlint check trust` (a non-flag subcommand before `trust`) must ALLOW
    // — `check` is the subcommand, not a flag, so scanning stops. Pins
    // `is_flag_token -> true`: under that mutant, `check` is mis-flagged,
    // skipped, and `trust` is reached → false block.
    #[test]
    fn allows_nonflag_subcommand_before_trust() {
        assert_allows("ironlint check trust");
    }

    // `cp .ironlint.yml /tmp/backup` via the multi-source form still allows
    // (policy path as SOURCE, not destination) — re-pin after dd/install work.
    #[test]
    fn allows_cp_from_ironlint_yml_still_allows() {
        assert_allows("cp .ironlint.yml /tmp/backup");
    }

    // `dd if=.ironlint.yml of=/tmp/backup` reads the policy file (source) and
    // writes a NON-policy destination — must allow.
    #[test]
    fn allows_dd_from_ironlint_yml_as_source() {
        assert_allows("dd if=.ironlint.yml of=/tmp/backup");
    }

    // --- gates → scripts rename ---
    #[test]
    fn allows_write_to_legacy_gates_path() {
        // After the gates→scripts rename, .ironlint/gates/ is no longer the
        // policy surface — a Bash write there is allowed (it's just a regular
        // repo directory now). Pin this so the matcher doesn't regress.
        assert_allows("echo x > .ironlint/gates/lint.sh");
    }

    // =====================================================================
    // v0.9.2 — `sh -c` descent + bare `VAR=val` prefix bypasses.
    // Both let a lazy non-reasoning model run `ironlint trust` through its
    // Bash tool despite the gate. See
    // plans/2026-07-07-bash-gate-sh-c-and-bare-env-bypasses.md.
    // =====================================================================

    // --- Task 1: `sh -c 'ironlint trust'` / `bash -c "ironlint trust"` ---
    // The shell runs the QUOTED argument as one command string (`ironlint
    // trust`), but normalize() strips quotes globally, so the gate was
    // analyzing `sh -c ironlint trust` (where sh runs only `ironlint`, and
    // `trust` becomes `$0`). strip_wrappers now descends into the `-c`
    // command-string argument and re-checks it.
    #[test]
    fn blocks_sh_c_ironlint_trust_single_quoted() {
        assert_blocks("sh -c 'ironlint trust'");
    }

    #[test]
    fn blocks_bash_c_ironlint_trust_double_quoted() {
        assert_blocks("bash -c \"ironlint trust\"");
    }

    // False-positive guard: a read-only subcommand under `sh -c` MUST still
    // allow. If the descent over-blocks (e.g. treats any `sh -c` content as a
    // trust invocation), this flips to Block.
    #[test]
    fn allows_sh_c_readonly_ironlint_check() {
        assert_allows("sh -c 'ironlint check'");
    }

    // --- Task 2: bare `VAR=val ironlint trust` ---
    // The `env VAR=val ironlint trust` form IS caught (env is a recognized
    // wrapper; skip_assignments drops VAR=val), but the semantically
    // equivalent bare prefix is not — strip_wrappers' fall-through `break`ed
    // on VAR=val before reaching the binary, and the first-token-is-binary
    // guard in is_ironlint_trust then bailed. The fix recognizes a leading
    // VAR=val assignment in the fall-through and skips it.
    #[test]
    fn blocks_bare_env_prefix_ironlint_trust() {
        assert_blocks("IRONLINT_ROOT=/x ironlint trust");
    }

    // Multiple leading assignments + trust — pins that the skip loop advances
    // past every VAR=val, not just the first.
    #[test]
    fn blocks_bare_env_prefix_multiple_assignments_trust() {
        assert_blocks("FOO=bar BAZ=qux ironlint trust");
    }

    // False-positive guard: a read-only subcommand with a bare env prefix MUST
    // still allow. `RUST_LOG=debug ironlint check` is a legit read.
    #[test]
    fn allows_bare_env_prefix_readonly_command() {
        assert_allows("RUST_LOG=debug ironlint check");
    }

    // Multiple leading assignments + a read-only subcommand MUST allow.
    #[test]
    fn allows_bare_env_prefix_multiple_assignments_readonly() {
        assert_allows("FOO=bar BAZ=qux ironlint check");
    }

    // --- is_assignment predicate pins (mutation-kill, not threat-model) ---
    // These pin the strictness of the shell-identifier check in is_assignment,
    // which exists to avoid over-skipping a non-assignment leading token.

    // A leading-underscore assignment is a VALID shell identifier — `_PRIVATE`
    // is a real env var name, so `_PRIVATE=1 ironlint trust` IS the bare-prefix
    // bypass and must Block. Pins the `c == '_'` in ok_first: under `!= '_'`,
    // `_PRIVATE=1` is misclassified as non-assignment, not skipped, and the
    // first-token-is-binary guard bails → false Allow.
    #[test]
    fn blocks_underscore_leading_assignment_trust() {
        assert_blocks("_PRIVATE=1 ironlint trust");
    }

    // A DIGIT-leading `=val` is NOT a valid shell assignment (identifiers
    // can't start with a digit), so sh runs `9x=1` as a command name (not
    // found) and `ironlint trust` never fires — the correct verdict is Allow.
    // Pins the `&&` in `ok_first && chars.all`: under `||`, `9x=1` is
    // misclassified as an assignment, skipped, and `ironlint trust` is
    // false-blocked. (Predicate-logic pin, not a realistic lazy-model form.)
    #[test]
    fn allows_digit_leading_non_assignment_prefix() {
        assert_allows("9x=1 ironlint trust");
    }

    // =====================================================================
    // v0.9.2 review follow-up: other shells with -c. `sh -c` descent was
    // scoped to sh/bash, but `dash` IS /bin/sh on Debian/Ubuntu (the
    // dominant Linux dev/CI env) — same binary, same threat. `ash` is the
    // BusyBox sh (Alpine, common in containers); `zsh` is the macOS default
    // interactive shell. A lazy model that knows its shell emits the
    // specific name. Bounded explicit list — same pattern as the existing
    // wrapper prefixes, not shell evaluation.
    // =====================================================================
    #[test]
    fn blocks_dash_c_ironlint_trust() {
        assert_blocks("dash -c 'ironlint trust'");
    }

    #[test]
    fn blocks_ash_c_ironlint_trust() {
        assert_blocks("ash -c 'ironlint trust'");
    }

    #[test]
    fn blocks_zsh_c_ironlint_trust() {
        assert_blocks("zsh -c 'ironlint trust'");
    }

    // False-positive guard: a read-only subcommand under `dash -c` MUST
    // still allow.
    #[test]
    fn allows_dash_c_readonly_ironlint_check() {
        assert_allows("dash -c 'ironlint check'");
    }

    // =====================================================================
    // Task 3: sibling-token regression pins. The `or`-confusion fix
    // (3e9cd0d) made is_ironlint_trust scan EVERY ironlint binary in a
    // segment, not just the first. These forms are caught today but
    // unpinned — a future "simplify back to first-only" would silently
    // reopen them. `and`/newline/comma are not shell separators, so
    // segments() leaves them as one segment; the second binary's `trust`
    // is what blocks.
    // =====================================================================
    #[test]
    fn blocks_ironlint_check_and_ironlint_trust() {
        assert_blocks("ironlint check and ironlint trust");
    }

    #[test]
    fn blocks_ironlint_check_newline_ironlint_trust() {
        assert_blocks("ironlint check\nironlint trust");
    }

    #[test]
    fn blocks_ironlint_check_comma_ironlint_trust() {
        assert_blocks("ironlint check, ironlint trust");
    }

    // =====================================================================
    // W3 — harness self-defense: adapter installation surface + git floor
    // (specs/2026-08-17-git-floor-hook-and-self-defense-design.md).
    // =====================================================================
    //
    // The adapter installation artifacts (settings files + plugin dirs the
    // `init` onboarding writes) and the W1 git pre-commit floor hook are
    // protected exactly like the ironlint policy surface. decide_with_home
    // is exercised with an explicit HOME so the ~/$HOME normalization is
    // pinned independent of the machine running the tests.

    // --- every surface FILE x every write op x every spelling ---
    #[test]
    fn blocks_redirect_to_home_claude_settings() {
        assert_blocks("echo x > ~/.claude/settings.json");
        assert_blocks("echo x > $HOME/.claude/settings.json");
        assert_blocks("echo x > /Users/t/.claude/settings.json");
        assert_blocks("echo x > .claude/settings.json");
        assert_blocks("echo x > sub/.claude/settings.json");
    }

    #[test]
    fn blocks_append_to_home_claude_settings() {
        assert_blocks("echo x >> ~/.claude/settings.json");
        assert_blocks("echo x >> $HOME/.claude/settings.json");
        assert_blocks("echo x >> /Users/t/.claude/settings.json");
        assert_blocks("echo x >> .claude/settings.json");
    }

    #[test]
    fn blocks_clobber_to_claude_settings_local() {
        assert_blocks("echo x >| ~/.claude/settings.local.json");
        assert_blocks("echo x >| .claude/settings.local.json");
        assert_blocks("echo x >| /Users/t/.claude/settings.local.json");
    }

    #[test]
    fn blocks_end_glued_redirect_to_claude_settings() {
        assert_blocks("echo x>~/.claude/settings.json");
        assert_blocks("echo x>$HOME/.claude/settings.json");
        assert_blocks("echo x>.claude/settings.json");
    }

    #[test]
    fn blocks_tee_to_codex_hooks() {
        assert_blocks("echo x | tee ~/.codex/hooks.json");
        assert_blocks("echo x | tee $HOME/.codex/hooks.json");
        assert_blocks("echo x | tee -a .codex/hooks.json");
        assert_blocks("echo x | tee /Users/t/.codex/hooks.json");
    }

    #[test]
    fn blocks_sed_inplace_on_surface_file() {
        assert_blocks("sed -i s/x/y/ ~/.claude/settings.json");
        assert_blocks("sed -i s/x/y/ .claude/settings.local.json");
        assert_blocks("perl -i -pe s/x/y/ $HOME/.codex/hooks.json");
    }

    #[test]
    fn blocks_cp_mv_install_rsync_onto_surface() {
        assert_blocks("cp /tmp/backup ~/.claude/settings.json");
        assert_blocks("mv /tmp/backup .claude/settings.json");
        assert_blocks("install /tmp/backup $HOME/.claude/settings.json");
        assert_blocks("rsync /tmp/backup /Users/t/.codex/hooks.json");
    }

    #[test]
    fn blocks_dd_rm_chmod_on_surface() {
        assert_blocks("dd if=x of=~/.claude/settings.json");
        assert_blocks("rm ~/.claude/settings.json");
        assert_blocks("rm -rf .claude/settings.local.json");
        assert_blocks("chmod 777 $HOME/.claude/settings.json");
        assert_blocks("chmod +x .codex/hooks.json");
    }

    // --- every surface DIR x every spelling ---
    #[test]
    fn blocks_writes_under_pi_extensions() {
        assert_blocks("echo x > .pi/extensions/foo.js");
        assert_blocks("tee x .pi/extensions/foo.js < /dev/null");
        assert_blocks("rm -rf .pi/extensions");
        assert_blocks("echo x > /Users/t/.pi/extensions/foo.js");
        assert_blocks("chmod 777 .pi/extensions/hook.js");
    }

    #[test]
    fn blocks_writes_under_home_pi_agent_extensions() {
        assert_blocks("echo x > ~/.pi/agent/extensions/foo.js");
        assert_blocks("echo x > $HOME/.pi/agent/extensions/foo.js");
        assert_blocks("rm -rf ~/.pi/agent/extensions");
        assert_blocks("echo x > /Users/t/.pi/agent/extensions/hook.js");
    }

    #[test]
    fn blocks_writes_under_opencode_plugins() {
        assert_blocks("echo x > .opencode/plugins/plugin.js");
        assert_blocks("rm -rf .opencode/plugins");
        assert_blocks("sed -i s/x/y/ .opencode/plugins/index.js");
    }

    // --- git floor hook (W1) + git-bypass forms (W3-R3) ---
    #[test]
    fn blocks_writes_to_git_floor_hook() {
        assert_blocks("rm .git/hooks/pre-commit");
        assert_blocks("rm -f /Users/t/repo/.git/hooks/pre-commit");
        assert_blocks("chmod -x .git/hooks/pre-commit");
        assert_blocks("echo x > .git/hooks/pre-commit");
        assert_blocks("cp /tmp/other .git/hooks/pre-commit");
    }

    #[test]
    fn blocks_git_commit_no_verify() {
        assert_blocks("git commit --no-verify -m x");
        assert_blocks("git commit -m x --no-verify");
    }

    // Review finding (2026-08-20): wrapper prefixes (`env`, leading VAR=val,
    // `nohup`, `timeout`, `sh -c`) must not shelter the bypass — the detector
    // strips wrappers before tokenizing.
    #[test]
    fn blocks_git_commit_no_verify_through_wrappers() {
        assert_blocks("env git commit --no-verify -m x");
        assert_blocks("FOO=1 git commit --no-verify -m x");
        assert_blocks("nohup git commit --no-verify -m x");
        assert_blocks("nohup git -c core.hooksPath=/t commit");
        assert_blocks("timeout 10 git commit -n -m x");
        assert_blocks("sh -c 'git commit --no-verify -m x'");
        assert_blocks("exec git commit --no-verify -m x");
    }

    #[test]
    fn blocks_git_c_core_hooks_path() {
        assert_blocks("git -c core.hooksPath=/tmp/x commit -m x");
        assert_blocks("git -c core.hooksPath=/tmp/x status");
    }

    #[test]
    fn blocks_git_config_core_hooks_path() {
        assert_blocks("git config core.hooksPath /tmp/x");
        assert_blocks("git config core.hooksPath /tmp/x && git commit -m y");
        assert_blocks("git config --unset core.hooksPath");
    }

    // Review finding (2026-08-20): mutating `git config` flags block only
    // when the key IS core.hooksPath — `--unset user.name` is not a floor
    // re-route and must stay allowed.
    #[test]
    fn allows_git_config_mutations_on_other_keys() {
        assert_allows("git config --unset user.name");
        assert_allows("git config --add alias.st status");
        assert_allows("git config --unset-all alias.st");
        assert_allows("git config --rename-section user foo");
    }

    // Review finding (2026-08-20): the GIT_CONFIG_* hookspath token only
    // injects env when it precedes the git binary; an echo of the string or
    // a config VALUE carrying it is not an injection.
    #[test]
    fn allows_git_config_env_token_outside_git_invocation() {
        assert_allows("echo GIT_CONFIG_KEY_0=core.hooksPath");
        assert_allows("git config alias.foo GIT_CONFIG_KEY_0=core.hooksPath");
    }

    // GIT_CONFIG_* env injection is git's env spelling of `-c`/`config`: the
    // KEY segment of the COUNT/KEY/VALUE triple carries `core.hooksPath`, so
    // block on the KEY (and PARAMETERS) token with no visible `-c`/`config`.
    #[test]
    fn blocks_git_config_env_injection() {
        assert_blocks(
            "env GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/tmp/x git commit -m x",
        );
        assert_blocks(
            "GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/tmp/x git commit -m x",
        );
        assert_blocks("GIT_CONFIG_KEY_0=core.hooksPath git commit -m x");
        assert_blocks("GIT_CONFIG_KEY_7=core.HooksPath GIT_CONFIG_COUNT=8 git commit -m x");
        assert_blocks("GIT_CONFIG_PARAMETERS='core.hooksPath=/tmp/x' git commit -m x");
        assert_blocks("GIT_CONFIG_PARAMETERS=core.hooksPath=/tmp/x git commit -m x");
    }

    // Narrow scope: a GIT_CONFIG_* var that does NOT name core.hooksPath is
    // not a floor re-route. COUNT/VALUE carry a count/path value; GLOBAL
    // redirects the global config FILE — all stay allowed, including a
    // hooksPath READ alongside an innocent GLOBAL redirect.
    #[test]
    fn allows_git_config_env_injection_without_hooks_path() {
        assert_allows("GIT_CONFIG_COUNT=1 git commit -m x");
        assert_allows("GIT_CONFIG_VALUE_0=/tmp/x git commit -m x");
        assert_allows("GIT_CONFIG_GLOBAL=/dev/null git status");
        assert_allows("GIT_CONFIG_GLOBAL=/dev/null git config --get core.hooksPath");
    }

    // Review finding (2026-08-17): `-n` is git's short form of `--no-verify`
    // for hook-running subcommands — an agent blocked on `--no-verify` just
    // re-emits `-n`. But `git log -n 5` (a count) must stay allowed.
    #[test]
    fn blocks_git_commit_dash_n_short_verify() {
        assert_blocks("git commit -n -m x");
    }

    // Round-2 review finding 5: only `commit`'s `-n` is `--no-verify`. For
    // merge/pull `-n` is `--no-stat`; for cherry-pick/revert it's
    // `--no-commit` — all benign. Long-form `--no-verify` must still block for
    // every hook-running subcommand (covered in the `blocks_git_commit_*_verify`
    // tests + below).
    #[test]
    fn allows_git_dash_n_for_non_commit_subcommands() {
        assert_allows("git merge -n");
        assert_allows("git pull -n");
        assert_allows("git cherry-pick -n");
        assert_allows("git revert -n");
    }

    #[test]
    fn blocks_git_no_verify_long_form_for_all_hook_subcommands() {
        assert_blocks("git commit --no-verify -m x");
        assert_blocks("git merge --no-verify");
        assert_blocks("git cherry-pick --no-verify");
        assert_blocks("git revert --no-verify");
        assert_blocks("git pull --no-verify");
    }

    // Round-2 finding 1: `git_subcommand` must skip value-taking flags
    // (`-c`/`-C`/`--config`) AND their values, or the `-n` arm checks the
    // flag's value (`user.name=x`) instead of `commit`.
    #[test]
    fn blocks_git_commit_dash_n_with_c_c_value_flags() {
        assert_blocks("git -c user.name=x commit -n -m x");
        assert_blocks("git -C /tmp commit -n");
        assert_blocks("git --config user.name=x commit -n -m x");
    }

    #[test]
    fn allows_git_log_dash_n_with_c_c_value_flags() {
        assert_allows("git -c user.name=x log -n 5");
        assert_allows("git -C /tmp log -n 5");
        assert_allows("git --config grep 'x' log -n 5");
    }

    // Round-2 finding 2: git-config keys are case-insensitive.
    #[test]
    fn blocks_git_config_hooks_path_case_insensitive() {
        assert_blocks("git config core.HooksPath /tmp/x");
        assert_blocks("git config core.HOOKSPATH /tmp/x");
        assert_blocks("git -c core.HooksPath=/tmp/x commit -m x");
        assert_blocks("git -ccore.HooksPath=/tmp/x commit -m x");
    }

    // Round-2 over-block 2: `--get` / bare key are READS — allowed.
    #[test]
    fn allows_git_config_get_hooks_path() {
        assert_allows("git config --get core.hooksPath");
        assert_allows("git config --get core.HooksPath");
        assert_allows("git config core.hooksPath");
    }

    #[test]
    fn blocks_git_config_hooks_path_mutating_flags() {
        assert_blocks("git config --add core.hooksPath /tmp/x");
        assert_blocks("git config --unset core.hooksPath");
        assert_blocks("git config --unset-all core.hooksPath");
    }

    // Round-2 finding 3: short-flag bundles are never scanned for `n`.
    // `-qn` = `-q -n`; but `-cuser.name=chris` (value after `c`) must NOT
    // false-block.
    #[test]
    fn blocks_git_commit_short_bundle_verify() {
        assert_blocks("git commit -qn -m x");
        assert_blocks("git commit -nv -m x");
    }

    #[test]
    fn allows_git_short_bundle_without_verify() {
        assert_allows("git -cuser.name=chris commit -m x");
        assert_allows("git commit -q -m x");
    }

    // Round-2 over-block 1: DIRECTORIES gate only the DELETION family. A
    // Bash-authored file under `.claude/` (a skill) is a legitimate edit, so
    // content-write families allow it — but exact FILES still block all
    // families and dir deletion still blocks.
    #[test]
    fn allows_bash_writes_under_profile_surface_dirs() {
        assert_allows("echo hi > .claude/agents/foo.md");
        assert_allows("cp skill.md sub/.claude/agents/out.md");
        assert_allows("echo x | tee .claude/skills/foo.md");
        assert_allows("mkdir -p .claude/skills/x");
    }

    #[test]
    fn allows_mv_into_profile_surface_dir_destination() {
        assert_allows("mv /tmp/foo .claude/agents/foo.md");
    }

    // The EXEC surfaces (.pi/extensions, .opencode/plugins) stay protected
    // for ALL write families (W3 self-trust: writing there installs a
    // rail-bypass executable).
    #[test]
    fn blocks_bash_writes_under_exec_surface_dirs() {
        assert_blocks("echo x > .pi/extensions/hook.js");
        assert_blocks("echo x > ~/.pi/agent/extensions/skill/foo.md");
        assert_blocks("cp /tmp/plugin.js .opencode/plugins/plugin.js");
        assert_blocks("mv /tmp/x .pi/extensions/y");
        assert_blocks("mv ~/.pi/agent/extensions /tmp/x");
    }

    #[test]
    fn blocks_exact_surface_files_all_families() {
        assert_blocks("echo x > .claude/settings.json");
        assert_blocks("echo y >> $HOME/.codex/hooks.json");
        assert_blocks("cp /tmp/x ~/.claude/settings.json");
    }

    #[test]
    fn blocks_dir_deletion_family() {
        assert_blocks("rm -rf .claude/agents");
        assert_blocks("chmod -x .claude/skills");
        assert_blocks("rmdir .codex");
        assert_blocks("rm -rf ~/.pi/agent/extensions");
    }

    // Round-2 finding 4: the floor hook's PARENT dir (`.git/hooks`) deletion
    // removes the rail without naming pre-commit — block it (deletion family
    // only, like the adapter surface dirs).
    #[test]
    fn blocks_floor_hooks_parent_dir_deletion() {
        assert_blocks("rm -rf .git/hooks");
        assert_blocks("rm -rf .git/hooks/");
        assert_blocks("rm -rf /repo/.git/hooks");
    }

    #[test]
    fn allows_git_log_dash_n_count() {
        assert_allows("git log -n 5");
        assert_allows("git show -n 3");
        assert_allows("git diff -n");
    }

    // Review finding (2026-08-17): `mv` with a protected SOURCE moves the
    // rail away — as destructive as a protected destination. cp stays
    // source-read-only.
    #[test]
    fn blocks_mv_from_surface_source() {
        assert_blocks("mv ~/.claude/settings.json /tmp/x");
        assert_blocks("mv .claude/settings.local.json /tmp/x");
        assert_blocks("mv ~/.pi/agent/extensions /tmp/x");
    }

    // Review finding (2026-08-17): unlink/truncate are rm-family deletes;
    // parent-dir deletion (rm -rf ~/.claude) and glob spellings of the floor
    // hook must block too.
    #[test]
    fn blocks_unlink_and_truncate_on_surface() {
        assert_blocks("unlink ~/.claude/settings.json");
        assert_blocks("truncate -s 0 ~/.claude/settings.json");
        assert_blocks("truncate -s 0 .codex/hooks.json");
    }

    #[test]
    fn blocks_parent_dir_deletion() {
        assert_blocks("rm -rf ~/.claude");
        assert_blocks("rm -rf ~/.codex");
        assert_blocks("rm -rf .claude");
    }

    #[test]
    fn blocks_floor_hook_glob_spelling() {
        assert_blocks("rm .git/hooks/pre-commit*");
        assert_blocks("rm -f /repo/.git/hooks/pre-commit*");
    }

    // --- allow pins: reads and lookalikes must NOT block ---
    #[test]
    fn allows_reading_surface_files() {
        assert_allows("cat ~/.claude/settings.json");
        assert_allows("cat $HOME/.claude/settings.json");
        assert_allows("cat .claude/settings.json");
        assert_allows("jq . ~/.codex/hooks.json");
    }

    #[test]
    fn allows_surface_path_as_source() {
        assert_allows("cp ~/.claude/settings.json /tmp/backup");
        assert_allows("cp .codex/hooks.json /tmp/backup");
        assert_allows("dd if=.claude/settings.json of=/tmp/backup");
    }

    #[test]
    fn allows_unrelated_settings_paths() {
        assert_allows("echo x > ./config/settings.json");
        assert_allows("echo x > /etc/app/settings.json");
        assert_allows("echo x > .pi/extensions-backup/foo.js");
        assert_allows("echo x > .pi/extensions_old/foo.js");
        assert_allows("echo x > my.opencode/plugins/x");
        assert_allows("echo x > .git/hooks/pre-commit.sh");
        assert_allows("echo x > .git/hooks/pre-commit.d/foo");
    }

    #[test]
    fn allows_plain_git_commands() {
        assert_allows("git commit -m x");
        assert_allows("git push origin main");
        assert_allows("git config user.name test");
        assert_allows("git status");
    }

    // --- decide_with_home: explicit HOME normalization parity with the
    //     env-driven decide() ---
    #[test]
    fn decide_with_home_blocks_home_spellings_with_explicit_home() {
        let home = "/h/u/m";
        for cmd in [
            "echo x > ~/.claude/settings.json",
            "echo x > $HOME/.claude/settings.json",
            "echo x > /h/u/m/.claude/settings.json",
            "echo x > ~/.pi/agent/extensions/foo.js",
            "echo x > $HOME/.pi/agent/extensions/foo.js",
            "rm -rf ~/.pi/agent/extensions",
            "rm ~/.codex/hooks.json",
        ] {
            assert!(
                matches!(decide_with_home(cmd, Some(home)), Decision::Block(_)),
                "expected Block for {cmd:?}"
            );
        }
    }

    #[test]
    fn decide_with_home_blocks_project_scoped_forms_without_home() {
        // Project-scoped install targets are surface regardless of any HOME:
        // `.claude/settings.json` (settings.local), `.codex/hooks.json`,
        // `.opencode/plugins/...` must all block with home=None too.
        for cmd in [
            "echo x > .claude/settings.json",
            "echo x > .claude/settings.local.json",
            "echo x > .codex/hooks.json",
            "echo x > sub/.opencode/plugins/foo.js",
            "echo x > .pi/extensions/foo.js",
        ] {
            assert!(
                matches!(decide_with_home(cmd, None), Decision::Block(_)),
                "expected Block for {cmd:?}"
            );
        }
    }
}
