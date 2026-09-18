# `ironlint init` clean-room test

This opt-in Docker test exercises the v1 first-run path on a real filesystem:

1. detect and install the supported Pi adapter;
2. scaffold and locally trust a `version: 1` policy;
3. run a complete acceptance check successfully; and
4. inspect the installation with `ironlint doctor`.

Run it from the repository root:

```sh
bash tests/e2e/init/run.sh
```

Docker is required. The first run builds IronLint in the Linux image; later
runs reuse Docker's build cache. Results are written under
`tests/e2e/init/runs/<timestamp>/`, including the isolated home, project,
acceptance verdict, doctor report, and container log.

The test is manual because it builds a full Linux image. The regular Rust and
Pi adapter suites remain the fast CI checks.
