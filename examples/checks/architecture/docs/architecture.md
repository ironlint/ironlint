# API import boundary

Modules under `src/project/api/` must call `project.services` for data access.
They must not import `project.db` directly. The AST check handles absolute
`import` and `from` forms, including multiline imports. Dynamic imports and
runtime aliases need another check if the project uses them.
