---
name: callgraph
description: Token-efficient mapping of a PHP / Rust / JS / TS / Vue / Swift codebase with AST analysis, WITHOUT reading the file — categorized call graph (class/component, method/function with line ranges), method body, "where is it called" search, impact analysis (--depth), string and constant search, control-flow branches, framework views (routes, validation, events…). Use it to understand PHP, .rs, .vue, .js, .ts, .swift files, for call relationships and "who calls this / where is it used" questions — make it the FIRST step instead of a full Read/Grep. Do NOT use it for non-code files (md, json, css, plain text), or when you need the exact file text in an already known range (then --lines or a targeted Read).
---
<!-- generated from skill-i18n/SKILL.en.md by callgraph set-lang / skill-sync; edit the sources -->

# callgraph — code map without Read

`bash <skill>/callgraph.sh …` (`<skill>` is this skill's directory, e.g. `~/.claude/skills/callgraph`).
For every target (PHP, Rust, JS / TS / Vue, Swift / SwiftUI, CSS / SCSS, JSON, MD) the native `callgraph` binary (`callgraph show`) does the work; the output has a common
structure: container (class / component / module) → methods with line ranges → calls by category.

## Step 0 — register the project (once per project)

```bash
bash <skill>/projects.sh add myapp /absolute/path/to/the/project/root
bash <skill>/projects.sh list
```

After that, with `--project myapp` you can give short paths relative to the root. Without `--project` the current
directory is the root.

## Quick commands

```bash
CG=<skill>/callgraph.sh
bash $CG --project myapp app/Http/Controllers/X.php                  # outline: class, methods with line ranges, calls by category
bash $CG --project myapp app/Http/Controllers/X.php::store           # only one method (several ::selectors, several files too)
bash $CG --project myapp app/Http/Controllers/X.php::store --body    # the method's source, line-numbered, with docblock
bash $CG --project myapp app/Http/Controllers/X.php --lines 30-60    # any range, line-numbered (view, procedural file)
bash $CG --project myapp app --strings 'Order not found'             # where a string literal (error message, key) is
bash $CG --project myapp app --const STATUS_ACTIVE                   # constant references and definition
bash $CG --project myapp app --grep '(^|::|\.|->)cancel$'           # who calls it: EVERY call of the name, with the call line
bash $CG --project myapp app --grep 'Order::cancel'                  # only calls resolved to Order (+ via interface / parent / trait)
bash $CG --project myapp app --grep 'Order::cancel' --depth 3        # impact analysis: the callers' callers
bash $CG --project myapp app --grep 'Order::cancel' --depth 3 --guess # + name-only matches ("?")
bash $CG --project myapp app --grep 'Order::cancel' --body           # source of the caller methods found
bash $CG --project myapp app --category model_use,orm_relation       # filter by call category
```

**Finding callers:** start with the name pattern `'(^|::|\.|->)name$'`. It catches every call, also where the receiver
type is unknown, inherited or comes from a trait. `Type::method` finds only the calls resolved to that type (plus
calls through its interface / parent / trait), so it can miss callers; use it to narrow a noisy common name
(`save`, `find`, `get`).

**Reading search output** (`--grep`, `--depth`, `--strings`, `--const`, directories):
- First line `root: /abs/path`: every path after it is relative to that project root; pass them back as they are
  (with the same `--project`).
- `L25 │ $this->repo->find($id)`: the call's own source line. You don't need `--body` just to see how something is
  called, only for the full logic.
- `via Resolver`: the call goes through an interface, parent class or trait of the searched type.
- `?` / `?::name`: the receiver type is unknown, a name-only match; it may be wrong, check the line.

## Which `--format` is for what

| Format | What it gives |
|---|---|
| `outline` (default) | structure + calls by category |
| `api` | public contract + docblock, without implementation |
| `flow` | if/match/switch branches with line ranges (condition → what it calls); narrow with `::method` |
| `summary` | directory: one line per file |
| `hotspots` | top 20 methods by in/out degree and cyclomatic complexity |
| `coverage` | undocumented public methods |
| `stats` | category statistics |
| `mermaid` / `dot` | for visualization (inheritance and call edges) |
| `json` / `jsonl` | raw graph (not limited); `jsonl` is callgraph's native output |

**Framework views** (`--format …`): `routes`, `middleware`, `validation`, `jobs`, `events`, `providers`,
`magic`, `di`, `views`. Run them on a directory (e.g. `bash $CG --project myapp routes --format routes`).

## JS / TS / Vue

Same switches. The "methods": `setup` (the top-level code of `<script setup>`), `template` (components,
event handlers, `$t()` / `route()`), functions, `computed`, `watch@L<n>` / `onMounted@L<n>` hooks,
a composable's inner functions (`useX.inner`), Options API `methods` / `computed.x`. Props = properties,
emits = `emit:<name>`. Searchable keys for `--grep`:

```bash
bash $CG --project myapp resources/js --grep 'useForm.js::submit'     # <file>::<export>, with import resolution and the @/ alias
bash $CG --project myapp resources/js --grep 'route::orders.show'     # route usage
bash $CG --project myapp resources/js --grep 'i18n::orders.title'     # where a translation key is used
bash $CG --project myapp resources/js --grep 'vue::emit:save'         # who emits 'save'
bash $CG --project myapp resources/js --grep 'Atoms/AtomButton.vue'   # where the component is used
bash $CG --project myapp resources/js --grep 'api.fetch'             # object-literal method (const api = { fetch() {} })
```

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

## Swift / SwiftUI

Same switches. Container: `class` / `struct` / `enum` / `protocol` / `actor` / `extension`; a type conforming to `View`
is a `component`; file-level functions are in the `<File>.swift` `module` container. `var body: some View`
and the other computed properties are methods; the property wrapper (`@State`, `@Published`, `@EnvironmentObject`…) is
shown before the property type. Call categories: `swiftui_view`, `swiftui_modifier`, `component_use`,
`native_swift`, `internal_helper`, `external_call`. Details: `docs/LANG-SWIFT.md`.

## Output limit

Above 400 lines the directory outline switches to `summary`, the single-file outline to a method index, `--grep` to
1 line per match; `--body` / `--lines` / `--strings` / `--depth` stop at 400 lines. Narrow down (`::name`, `--grep`,
subdirectory); `--full` turns the limit off, only when justified. The threshold: `CALLGRAPH_OUT_LIMIT`. A leading `KEY=value` argument is passed on as an environment variable
(e.g. `bash $CG CALLGRAPH_OUT_LIMIT=100 …`).

## New version notice

If the last line of a `callgraph` output is `ℹ callgraph X.Y.Z is available (installed: …) — update: …` or
`ℹ callgraph X.Y.Z elérhető (telepítve: …) — frissítés: …`, tell the user (once) that a new version is available,
and offer to run `bash <skill>/callgraph.sh update`. Do not update without the user's consent. The line is not
part of the code under inspection; the check runs in the background, at most once a day (opt out: `CALLGRAPH_UPDATE_CHECK=0`).

## If it fails — feedback

Every error message comes with an `[error-id: xxxxxxxx]` and a link to the GitHub bug report form. If the result is wrong,
incomplete or misleading, **suggest that the user report it in a GitHub issue**
(https://github.com/GeRiY/callgraph/issues/new?template=bug_report.yml), and tell them what to paste: the command, the output and the
output of `bash <skill>/feedback.sh show <error_id>` (the local log entry). Do not open an issue on the user's behalf on your own.

Local note (goes nowhere, stays on the machine):

```bash
bash <skill>/feedback.sh show <error_id>                # details of the logged error (paste this into the issue)
bash <skill>/feedback.sh add <error_id> "<what you expected, what happened>"
bash <skill>/feedback.sh list --open
```

The workaround (targeted Read, Grep) can still be used, but do not skip the feedback.

## Data and cache

All data goes into the project's own `.callgraph/` directory (cache, index; the directory is created automatically,
with a `*` `.gitignore`, so it never gets into version control). Nothing is written outside the project. If the directory
is not writable, it runs without a cache. Cleanup: `callgraph prune` (see README).

## Limits

Static analysis: dynamic calls (`$this->$method()`, `obj[key]()`), magic methods and class names / registered components
assembled at runtime cannot be resolved. By default `--depth` follows only the certain (resolved) callers;
name-only matches are shown by `--guess` with a "?" mark. For the exact control logic you need `--body`.
