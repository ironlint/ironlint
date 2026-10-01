# Check recipes

These commands run once against the project tree. Review and adapt them before
granting execution consent.

## Runnable Python examples

The [example catalog](../../examples/checks/README.md) contains three v1
policies with Python 3.11+ scripts: a `project.api` to `project.db` import
boundary, a forbidden `requests` import from `project.core`, and a canonical
generated JSON artifact. Each has passing, failing, and repaired fixtures.
Build `ironlint`, then run `IRONLINT_TEST_BIN=/absolute/ironlint bash
scripts/test-check-recipes.sh` from the repository root. The runner uses a
temporary consent store and verifies all nine acceptance outcomes.

## Rust workspace

```yaml
version: 1
checks:
  format:
    files: ["*.rs", "Cargo.toml"]
    on: [change, accept]
    run: cargo fmt --all --check
  tests:
    run: cargo test --locked
  clippy:
    run: cargo clippy --locked --all-targets -- -D warnings
```

## Project-owned command

```yaml
version: 1
checks:
  architecture:
    files: ["src/**", ".ironlint/scripts/architecture.sh"]
    on: [change, accept]
    run: sh .ironlint/scripts/architecture.sh
```

The script selects inputs and returns nonzero on violation. Use its normal error
handling so a failed step cannot be hidden by later success. Changes to a managed
script require renewed execution consent.

## Acceptance-only check

```yaml
version: 1
execution:
  timeout_secs: 120
  total_timeout_secs: 300
checks:
  integration:
    run: sh scripts/integration-tests.sh
```

Omitting `on` defaults to acceptance. An expensive command remains required
before acceptance without repeating on each edit.
