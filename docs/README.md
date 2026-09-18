# IronLint documentation

IronLint is a local, policy-based check runner for projects that use AI coding
tools. A policy lists the commands your project wants to run; IronLint selects
them for a changed path or runs the complete set before you accept work.

## Start here

- [Getting started](getting-started.md): install from source, create a policy, and run it.
- [Writing checks](writing-checks/README.md): define reliable project commands.
- [Check recipes](writing-checks/recipes.md): starting points for common projects.
- [Execution consent](security/trust.md): review commands before granting local permission.

## Use IronLint

- [Running checks](operating/running-checks.md): choose `change` or `accept`.
- [Targeting files](configuring/targeting-files.md): control early feedback.
- [Inspecting a policy](operating/inspecting-config.md) and [diagnostics](operating/diagnostics.md).
- [JSON results](reference/verdict-json.md), [policy reference](reference/config-schema.md), and [CLI reference](reference/cli.md).

## Optional integrations

- [AI-tool integrations](adapters/README.md): adapter behavior and safe removal.
- [How IronLint works](architecture.md): the policy evaluation model and its limits.
