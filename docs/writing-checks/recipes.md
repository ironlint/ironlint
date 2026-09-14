# V1 check recipes

These commands run once against the actual tree. Review and adapt them to your
project before granting execution consent.

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
handling so a failed step cannot be hidden by later success. Managed script changes
require renewed consent; external acceptance separately fixes approved provenance.

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
