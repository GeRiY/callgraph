# callgraph — Python

`CG=<skill>/callgraph.sh`, as in [SKILL.md](../SKILL.md) (the core: start there).

## Python

Same switches. File-level functions and constants are in the `<file>.py` `module` container; `class` with bases
(`extends`) and decorators (`uses @dataclass`). Method kinds: `method` / `staticmethod` / `classmethod` / `property`
(`async ` prefix); `self` / `cls` is left out of the signature; a leading `_` = private. Call categories: `native_python`,
`internal_helper`, `static_call`, `instantiation`, `external_call`, `dynamic_call` (`getattr(obj, name)()`, `globals()[name]()`,
`import_module(name)`, `eval` / `exec`). Details: `docs/LANG-PYTHON.md`.
