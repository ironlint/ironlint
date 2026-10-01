# Tested check examples

These examples use v1 YAML and Python 3.11+ standard-library scripts. Copy a
policy and its matching `.ironlint/scripts/` file only after reviewing the rule
and adapting paths to your project. `ironlint trust` is a separate owner action.
The example scripts belong to the project being checked; a controlled acceptance
owner that needs independent enforcement must pin approved scripts outside
candidate control.

| Example | Check ID | Scope | Limitation |
| --- | --- | --- | --- |
| [Architecture](architecture/.ironlint.yml) | `architecture` | Python AST absolute imports in `src/project/api/` | Dynamic imports and aliases assigned at runtime are not resolved. |
| [Dependency](dependency/.ironlint.yml) | `dependency` | Python AST absolute imports in `src/project/core/` | Runtime `__import__` and indirect package access are not resolved. |
| [Generated JSON](generated/.ironlint.yml) | `generated` | Exact canonical JSON generated from `data/catalog.json` | The example generator is JSON canonicalization only. |

The import checks parse each source file, including multiline imports and
ordinary import aliases. A missing source directory or parse failure blocks.
The generated check rejects missing or invalid JSON. Each diagnostic gives a
file/location or artifact path, the required pattern, and a project reference.

Build the CLI, then run:

```sh
IRONLINT_TEST_BIN=/absolute/path/to/ironlint bash scripts/test-check-recipes.sh
```

The runner copies each policy/script with passing, failing, and repaired
fixtures into temporary Git roots and uses a temporary `XDG_CONFIG_HOME`; it
does not touch the user's consent store. Its expected IDs and exit statuses are
checked against the real CLI JSON result.
