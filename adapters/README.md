# Independently released adapters

`ironlint-core` owns policy evaluation. `crates/ironlint-adapters` owns generic
installation and ownership; `ironlint init` and `doctor` expose that service.
Harness scripts and registrations are runtime package files, not evaluator build
inputs. Each harness has its own version and compatibility record.

```text
crates/ironlint-core/       evaluator
crates/ironlint-cli/        CLI and read-only diagnostics
crates/ironlint-adapters/   installation, recovery, ownership
adapters/shared/           bounded hook runner, packaging, authoring skill
adapters/pi/               Pi package and controlled completion
adapters/codex/            Codex package and native hooks
adapters/claude-code/      Claude Code package and native hooks
```

Codex and Claude Code use native plugin manifests plus hooks/hooks.json.
Their shared Python code is copied into self-contained archives during packaging;
source files remain single-source under shared/hooks. Pi retains its npm package.
The Rust installer consumes the same native hook manifests at installation time.
All packages depend on the stable v1 CLI/schema-7 contract and require evaluator
1.1.0+ for cooperative cancellation.

Build one package without building Rust:

```sh
python3 adapters/shared/pack.py codex --output /tmp/ironlint-packages
python3 adapters/shared/pack.py claude-code --output /tmp/ironlint-packages
python3 adapters/shared/pack.py pi --output /tmp/ironlint-packages
python3 adapters/shared/pack.py claude-code --plugin-only --output /tmp/ironlint-packages
```

An archive contains the selected harness directory and its shared installation
inputs. After extracting, set IRONLINT_ADAPTERS_ROOT to that parent directory and
run `ironlint init --harness <name>`. The native harness directory itself is also
a self-contained plugin, including its Python helpers and configuration skill.
For native plugin managers, `--plugin-only` emits a separate
`ironlint-<harness>-plugin-<version>.zip` with plugin content at the ZIP root and
no sibling shared directory. Use the regular archive for `ironlint init`.
Increment the harness's adapter.json version (Pi uses package.json) when shipping
an update. Versioning does not change the evaluator, and a package update needs
only extraction and reinstall. Packages are optional separate downloads for
source-installed and release-installed binaries.

## Native release distribution

The prepared Claude Code marketplace catalog lives at
`.claude-plugin/marketplace.json`. It references a versioned GitHub Release plugin ZIP
in `ironlint/ironlint`, using the `adapter-claude-code-v0.1.0` tag and the
archive's SHA-256. Marketplace installation requires Claude Code 2.1.224+.
Older clients use extracted packages with `ironlint init`. The marketplace
retains its catalog identity, migrates the legacy plugin name, and gets its
plugin version from the packaged manifest. Catalog versions are independent
of adapter versions.

For each native release, update the adapter and plugin manifest versions
together, build the plugin-only package, compute its SHA-256, then update the catalog URL
and pin to that exact artifact. Upload the exact pinned ZIP; rebuilding it
requires recomputing the pin. Use `adapter-<harness>-v<version>` tags so adapter
releases bypass the evaluator's cargo-dist workflow. The adapter workflow only
builds CI artifacts; creating tags and uploading releases are separate,
authorized operator actions. For Codex, distribute its independently versioned
ZIP as a separate release download.

The current release artifacts and catalog URL are prepared locally. No tag,
release upload, or marketplace installation is claimed. Publication destination
still awaits the operator's choice.

Run native contract/package tests with Python unittest and Pi tests with npm.
`scripts/ci-adapters.sh` builds a local evaluator, then runs both suites with
isolated policy consent. Live harness/model qualification belongs to each
adapter's compatibility record, outside the core release gate. Native Stop
hooks are local repair aids; immutable candidate completion remains separately
qualified under Pi's controlled runner.
