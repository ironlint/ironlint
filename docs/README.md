# IronLint documentation

These pages describe the current checkout. V1 core/CLI evaluation is implemented;
external acceptance, completed-edit feedback, and old-installation cleanup remain
in progress. The [architecture](architecture.md) identifies existing behavior;
the [active plan](../plans/2026-09-05-ironlint-v1-implementation.md) tracks remaining work.

## Use the implemented v1 evaluator

- [Getting started](getting-started.md): build, write a policy, validate, consent, evaluate.
- [Writing checks](writing-checks/README.md) and [recipes](writing-checks/recipes.md).
- [Config schema](reference/config-schema.md) and [trigger paths](configuring/targeting-files.md).
- [Running checks](operating/running-checks.md), [CLI](reference/cli.md), and [verdict JSON](reference/verdict-json.md).
- [Inspecting config](operating/inspecting-config.md), [resolved output](reference/show-resolved-config.md), and [diagnostics](operating/diagnostics.md).
- [Execution consent](security/trust.md).

## Installation and other current surfaces

- [Adapters](adapters/README.md): existing installation status and v1 support gaps.
- [Telemetry](operating/telemetry.md) and [watch](operating/watching-checks.md):
  existing log consumers; v1 evaluation does not currently write their records.

## Develop v1

- [Architecture](architecture.md): current code and responsibilities.
- [V1 contract](../specs/2026-09-05-ironlint-v1-design.md): release semantics.
- [Implementation plan](../plans/2026-09-05-ironlint-v1-implementation.md): work packets and evidence.
- [Agent instructions](../AGENTS.md): working rules.
