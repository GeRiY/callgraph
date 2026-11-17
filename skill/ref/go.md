# callgraph — Go

`CG=<skill>/callgraph.sh`, as in [SKILL.md](../SKILL.md) (the core: start there).

## Go

Same switches. File-level functions and package constants / variables are in the `<file>.go` `module` container;
a `struct` is a `class` (fields = properties, embedded types in `extends`), an `interface` is an `interface` (methods
abstract), `type X int` is a `type`, `type A = B` an `alias`. A method with a receiver (`func (r *Repo) Find`) belongs
to the `Repo` type, even if it is declared in another file. Capitalised name = `public`, lower case = `private`.
`--signatures` shows several return values (`Find(id int): (*User, error)`). Call categories: `native_go`
(builtins, standard library), `internal_helper`, `static_call`, `instantiation`, `external_call`. `_test.go` files
rank as tests in `locate`; `vendor` and `testdata` are skipped. Details: `docs/LANG-GO.md`.
