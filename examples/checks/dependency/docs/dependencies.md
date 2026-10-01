# Core dependency boundary

Modules under `src/project/core/` must use `project.adapters.http` for HTTP.
They must not import `requests` directly. The AST check catches absolute
`import requests as ...` and `from requests...` forms. Dynamic imports and
indirect dependencies need another check if they matter to the project.
