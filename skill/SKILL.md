---
name: callgraph
description: Token-efficient mapping of a PHP / Blade / Rust / JS / TS / Vue / Swift / Kotlin / Python / Go codebase with AST analysis, WITHOUT reading the file — where to start with a task (locate: ranked files from the names in the task), categorized call graph (class/component, method/function with line ranges), method body, "where is it called" search, impact analysis (--depth), string and constant search, control-flow branches, framework views (routes, validation, events…). Use it to understand PHP, .blade.php, .rs, .vue, .js, .ts, .swift, .kt files, for the structure of md / json / css / scss / html files (sections, keys, rules, links), for call relationships and "who calls this / where is it used" questions — make it the FIRST step instead of a full Read/Grep. Do NOT use it for plain text files, or when you need the exact file text in an already known range (then --lines or a targeted Read).
---

# callgraph — code map without Read

`bash <skill>/callgraph.sh …` (`<skill>` is this skill's directory, e.g. `~/.claude/skills/callgraph`).
For every target (PHP / Blade, Rust, JS / TS / Vue, Swift / SwiftUI, Kotlin / Android, Python, Go, CSS / SCSS, JSON, MD, HTML) the native `callgraph` binary (`callgraph show`) does the work; the output has a common
structure: container (class / component / module) → methods with line ranges → calls by category.

## Step 0 — register the project (once per project)

```bash
bash <skill>/projects.sh add myapp /absolute/path/to/the/project/root
bash <skill>/projects.sh list
```

After that, with `--project myapp` you can give short paths relative to the root; `--project <dir>` (an existing
folder, absolute or relative) works without registering. Without `--project` the current directory is the root.

## Step 1 — where to start: `locate` (the first call of a task)

<!-- mcp:start -->
**Which tool for what:** plain text search (an exact string, log message, config value) → Grep, it is the best at
that; "where do I start with this task?" → `locate`; definitions, structure, callers, impact → callgraph (outline,
`--body`, `--grep`, `--depth`).
**The first call of a task:** give `locate` the names and domain terms of the task in ONE call: function, class,
property and file names, columns, routes, translation keys, and for a non-English domain word its English code name too (e.g.
visszatérítés → `refund`). It answers from the project map without reading files: the top 5 files (`--all`: every
match), each with its reasons (exact definition, file name, name part, string, call, `callers: N`). Continue on
those files with the outline / `--body` / `--grep`.
**Recipes:** bug fix = `locate` → `outline` of the top file → `--body` of the method. "Who calls it / what breaks" =
the name pattern with `--grep` and `--depth` 2 → `--body` of the calling method. Unfamiliar project = `profile` →
`locate` → `outline`. "Where is this text / constant" = `--strings` / `--const` (exact text: Grep).
<!-- /mcp -->
Special terms: a quoted phrase with spaces (`"Szervező további eseményei"`) is ONE term, matched as a whole against
string literals (UI labels, translations), never split into words; `route::events.show` finds the route definition
(`->name('events.show')` in `routes/*.php`, ranked first) and its `route()` / `to_route()` / `redirect()->route()`
calls in PHP and JS / TS / Vue; `--orange-500` is a CSS custom property, not a flag. Files with an exact definition or
file name rank before files that only match strings (translations, build output). A name that joins several terms
(`toggleFavorite` for `toggle favorite`, `PodcastEpisodeController` for `podcast episode`) ranks high even when each
term alone is too general; definitions in type declarations (`.d.ts`) weigh less than real ones.

```bash
bash <skill>/callgraph.sh locate RefundController processRefund refund payment_status checkout.vue --project myapp   # where to start: ranked files + reasons
bash <skill>/callgraph.sh locate refund --all --project myapp                                                       # every matching file, not just the top 5
bash <skill>/callgraph.sh locate route::events.show "Szervező további eseményei" --project myapp                     # a route (definition first) and a UI phrase
```

## Recipes — task → call sequence

Four typical tasks and the calls that answer them (an empty or wrong result comes with a hint: `no call target matches …`,
`Did you mean: …` — follow it instead of retrying blindly):

- **Fix a bug / change behaviour:** `locate <the names in the task>` → the outline of the top file
  (`callgraph.sh <file>`) → `callgraph.sh <file>::<method> --body` for just the method you need.
- **"Who calls this / what does it affect if I change it":** `callgraph.sh <dir> --grep '(^|::|\.|->)name$' --depth 2`
  (the name pattern catches every call) → `callgraph.sh <caller file>::<caller method> --body`.
- **Unfamiliar project:** `callgraph.sh profile` (architecture, layers) → `locate <terms>` → the outline of the files it
  lists → `--body`.
- **"Where is this text / constant":** `callgraph.sh <dir> --strings <fragment>` (UI text, messages; a case-insensitive
  substring) or `--const <name>`; for an exact text, or in plain-text files, use Grep.

## Quick commands

```bash
CG=<skill>/callgraph.sh
bash $CG --project myapp app/Http/Controllers/X.php                  # outline: class, methods with line ranges, calls by category
bash $CG --project myapp app/Http/Controllers/X.php::store           # only one method (several ::selectors, several files too)
bash $CG --project myapp app/Http/Controllers/X.php::store --body    # the method's source, compact (comments kept, docblock omitted: --doc; --raw = exact)
bash $CG --project myapp app/Http/Controllers/X.php --lines 30-60    # any range, line-numbered (view, procedural file)
bash $CG --project myapp package.json::scripts --body                # one JSON key with its exact lines (dot path: scripts.build, files.0)
bash $CG --project myapp app --strings 'Order not found'             # where a string literal (error message, key) is
bash $CG --project myapp app --const STATUS_ACTIVE                   # constant: definition, references, re-exports
bash $CG --project myapp app --grep '(^|::|\.|->)cancel$'           # who calls it: EVERY call of the name, with the call line
bash $CG --project myapp app --grep 'Order::cancel'                  # only calls resolved to Order (+ via interface / parent / trait)
bash $CG --project myapp app --grep 'Order::cancel' --depth 3        # impact analysis: the callers' callers
bash $CG --project myapp app --grep 'Order::cancel' --depth 3 --guess # + name-only matches ("?")
bash $CG --project myapp app --grep 'Order::cancel' --body           # source of the caller methods found
bash $CG --project myapp app --grep 'App\Svc\Order::cancel'        # PHP namespace: a single \ is read as the separator (App\\Svc\\ also works); a bad regex gets a 💡 hint
bash $CG --project myapp app --category model_use,orm_relation       # filter by call category
bash $CG --project myapp app --category dynamic_call                 # the calls whose target is decided at runtime
```

<!-- mcp:callers -->
**Finding callers:** start with the name pattern `'(^|::|\.|->)name$'`. It catches every call, also where the receiver
type is unknown, inherited or comes from a trait. `Type::method` finds only the calls resolved to that type (plus
calls through its interface / parent / trait), so it can miss callers; use it to narrow a noisy common name
(`save`, `find`, `get`).
<!-- /mcp -->

<!-- mcp:output -->
**Reading search output** (`--grep`, `--depth`, `--strings`, `--const`, directories):
- First line `root: /abs/path`: every path after it is relative to that project root; pass them back as they are
  (with the same `--project`).
- `L25 │ $this->repo->find($id)`: the call's own source line. You don't need `--body` just to see how something is
  called, only for the full logic.
- `via Resolver`: the call goes through an interface, parent class or trait of the searched type.
- `L12-30` after a name: its line range; for the body logic (conditions, exact order) read just that range (Read with offset / limit), not the whole file.
- `L12, L40 │ …`: the same call on several lines; at most 3 rows per caller, then `… +2 more: L55, L61`.
- `?` / `?::name`: the receiver type is unknown, a name-only match; it may be wrong, check the line.
- `🔒` after a file path: the file is protected; read it, don't edit it.
<!-- /mcp -->

## Project profile (start here in an unfamiliar project)

```bash
bash $CG profile --project myapp                                     # architecture, layers (folders, file counts), patterns, call direction, unknown folders
bash $CG profile --project myapp --full                              # per folder, confidence counts, the call pairs between layers
bash $CG --project myapp app --layer service                         # only the files of one layer (several: service,repository)
bash $CG --project myapp . --layer controller:js                     # one ecosystem only (php, js, swift, rust) when a role is in both
bash $CG --project myapp app --format summary --layers               # summary grouped by layer
bash $CG --project myapp app --grep 'Repo::find' --depth 2 --by-layer # callers grouped by layer, a "[service]  3 callers" header per group
```

In an unfamiliar project run `profile` once at the start (~15 lines) instead of listing folders: it names the layers
(`controller`, `service`, `repository`, `model`, `component`, `store`…) and the folders that hold them, so you can narrow a
query with `--layer <name>` (a layer name from the profile, not a path). `--layers` / `--by-layer` only group and order,
never hide results. MCP: the `project_profile` tool and the `layer` / `by_layer` parameters. Details (vendor code,
`.callgraph-layers`, role proofs): [ref/reference.md](ref/reference.md).

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

**Framework views** (`--format routes|middleware|validation|jobs|events|providers|magic|di|views`, and for Laravel +
Inertia `ziggy|forms|models|inertia`; in any project `i18n`, `classes`): see [ref/php.md](ref/php.md).

A `📝 <time> · <text>` line (or a trailing `📝 N`) is a note left by a person or an agent: read it before changing that
code; how to leave one: [ref/reference.md](ref/reference.md).

## Output limit

Above 400 lines the directory outline switches to `summary`, the single-file outline to a method index, `--grep` to
1 line per match; `--body` / `--lines` / `--strings` / `--depth` stop at 400 lines. Narrow down (`::name`, `--grep`,
subdirectory); `--full` turns the limit off, only when justified. The threshold: `CALLGRAPH_OUT_LIMIT`. A leading `KEY=value` argument is passed on as an environment variable
(e.g. `bash $CG CALLGRAPH_OUT_LIMIT=100 …`).

Static analysis: dynamic calls (`$this->$method()`, `obj[key]()`) and runtime-assembled names are not resolved (marked
`dynamic_call`); a `⚠ syntax error` line means the file parsed only partly: read that file directly.
Other limits, `--strict`, `--signatures`: [ref/reference.md](ref/reference.md).

## Read next (only what the task needs)

Check the languages in `callgraph profile` (or the extensions of the files in the task) and Read only the matching
file(s) from this skill's `ref/` directory:

- [ref/php.md](ref/php.md): PHP / Laravel / Inertia (receiver types, framework views `routes`, `validation`, `ziggy`, `i18n`…)
- [ref/blade.md](ref/blade.md): Blade (`*.blade.php`)
- [ref/js.md](ref/js.md): JS / TS (`--grep` keys, TS receiver types)
- [ref/vue.md](ref/vue.md): Vue (`setup`, `template`, emits, component usage)
- [ref/css.md](ref/css.md): CSS / SCSS / HTML
- [ref/rust.md](ref/rust.md): Rust
- [ref/swift.md](ref/swift.md): Swift / SwiftUI
- [ref/kotlin.md](ref/kotlin.md): Kotlin / Android (Compose)
- [ref/python.md](ref/python.md): Python
- [ref/go.md](ref/go.md): Go
- [ref/gen.md](ref/gen.md): only when writing code: `gen` / `rename` / `undo`
- [ref/mcp.md](ref/mcp.md): only when configuring or using the MCP server
- [ref/reference.md](ref/reference.md): rarely needed: profile details, data and cache, ignore files, new-version notice
  (a last output line `ℹ callgraph X.Y.Z …`), `[error-id: …]` feedback, notes, suggested next step, limits

MCP clients without file access: the `guide` tool with `topic` = `php`, `blade`, `js`, `vue`, `css`, `rust`, `swift`, `kotlin`, `python`, `go`, `gen`, `mcp` or
`reference` returns the same file (`guide {topic: "php"}`).
