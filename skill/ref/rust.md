# callgraph — Rust

`CG=<skill>/callgraph.sh`, as in [SKILL.md](../SKILL.md) (the core: start there).

## Rust

Container = the file module (`module`), `struct` / `enum` / `trait` / inline `mod`; the methods of `impl` blocks go
to the type (`implements` = the traits). Categories: `internal_helper` (own), `module_use` (another module of the crate, `resolved`
= `module::fn` / `Type::fn`), `native_rust` (std), `framework_use` (axum, actix-web, tokio, serde…), `package_use`
(another crate), `macro_use` (macro; its body is not expanded), `external_call` (unknown receiver).

```bash
bash $CG --project myapp src/api/users.rs                            # outline: struct/impl/trait, fns with line ranges
bash $CG --project myapp src --grep 'Repo::find'                      # who calls it (crate-internal: Type::fn / module::fn)
bash $CG --project myapp src --const MAX_ITEMS                        # const / static (also Type::CONST): definition + uses
bash $CG --project myapp src/api/users.rs --format api               # pub items, incl. the pub fn of impl blocks
```

A method called on a value (`x.load()`) shows as `Type::load` when the type of `x` is clear (parameter, `let x =
Type::new()`, `self`, own struct field), otherwise as `?::load`; a `Type::load` search lists those too, as uncertain.
