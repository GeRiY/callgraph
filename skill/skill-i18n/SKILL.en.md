---
name: callgraph
description: Token-efficient mapping of a PHP / Rust / JS / TS / Vue / Swift / Python / Go codebase with AST analysis, WITHOUT reading the file — where to start with a task (locate: ranked files from the names in the task), categorized call graph (class/component, method/function with line ranges), method body, "where is it called" search, impact analysis (--depth), string and constant search, control-flow branches, framework views (routes, validation, events…). Use it to understand PHP, .rs, .vue, .js, .ts, .swift files, for call relationships and "who calls this / where is it used" questions — make it the FIRST step instead of a full Read/Grep. Do NOT use it for non-code files (md, json, css, plain text), or when you need the exact file text in an already known range (then --lines or a targeted Read).
---

# callgraph — code map without Read

`bash <skill>/callgraph.sh …` (`<skill>` is this skill's directory, e.g. `~/.claude/skills/callgraph`).
For every target (PHP, Rust, JS / TS / Vue, Swift / SwiftUI, Python, Go, CSS / SCSS, JSON, MD) the native `callgraph` binary (`callgraph show`) does the work; the output has a common
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
**The first call of a task:** give `locate` the names and domain terms of the task in ONE call: function, class and
file names, columns, routes, translation keys, and for a non-English domain word its English code name too (e.g.
visszatérítés → `refund`). It answers from the project map without reading files: the top 5 files (`--all`: every
match), each with its reasons (exact definition, file name, name part, string, call, `callers: N`). Continue on
those files with the outline / `--body` / `--grep`.
**Recipes:** bug fix = `locate` → outline of the top file → `--body` of the method. "Who calls it / what breaks" =
the name pattern with `--grep` and `--depth` 2 → `--body` of the calling method. Unfamiliar project = `profile` →
`locate` → outline. "Where is this text / constant" = `--strings` / `--const` (exact text: Grep).
<!-- /mcp -->
Special terms: a quoted phrase with spaces (`"Szervező további eseményei"`) is ONE term, matched as a whole against
string literals (UI labels, translations), never split into words; `route::events.show` finds the route definition
(`->name('events.show')` in `routes/*.php`, ranked first) and its `route()` / `to_route()` / `redirect()->route()`
calls in PHP and JS / TS / Vue; `--orange-500` is a CSS custom property, not a flag. Files with an exact definition or
file name rank before files that only match strings (translations, build output).

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
  substring) or `--const <name>`; for an exact text, or in non-code files, use Grep.

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

In an unfamiliar project run `profile` once at the start (~15 lines) instead of listing folders: it says which folder
holds which layer, so you can narrow a query with `--layer <name>` without knowing the folder names. A layer is a name
from the profile (`controller`, `service`, `repository`, `model`, `component`, `store`…), not a path. Roles come from
folder and file names, confirmed by the graph (confidence high / medium / low; "unknown" when unclear). Layers are
listed by file count. When a role is in both the backend and the frontend, the profile shows them apart
(`controller (php)` / `controller (js)`) and the call direction never chains them; `--layer controller` matches both,
`--layer controller:js` (or `"controller (js)"`) one. Third-party code copied into the project (a subfolder with its
own `composer.json` / `package.json` / `LICENSE`, or `vendors/`, `third_party/`, `extensions/`, `external/`, `libs/`)
and build output (`dist/`, `build/`, `*.min/`, minified files) are left out of the profile — one `vendor: N files
skipped (…)` line says so; queries still see these files (`profile --with-vendor` counts them); a copied framework
core (CodeIgniter `system/`, Yii `framework/`, Kohana `system/`) counts as vendor code too. `event`, `listener`, `job`,
`mail` and `notification` need code proof (an Event / Listener base class or interface, `Dispatchable`, `ShouldQueue`,
`handle(…)` in a `Listeners` namespace, `extends Mailable` / `Notification`); a folder named `Event` alone is not an
event layer. An enum gets no role from its name (`PaymentProvider` is not a provider); an `Enums` folder is `enum`.
`--layer`, `--layers` and `--by-layer` classify exactly like the profile. The `flow:` line only links layers that
really call each other, side branches after `;` (`controller → service → model; command → service`). A big subtree
the profile should ignore (an old copy of the app, e.g. `sn_monitoring/`): a path line in `.callgraphignore`
(`sn_monitoring/`) — it then disappears from every query too. A layer never hides results by itself:
only `--layer` filters, `--layers` / `--by-layer` only group and order. `--by-layer` groups the calls (with `--depth`:
per level) by layer, the layer closest to the callee first (the deepest layer in the call direction: model →
repository → service → controller), each group under a `[service]  3 calls` header. Own folder names:
`.callgraph-layers` in the project root (`service = app/Logic`, `ignore-role = app/Legacy`). MCP: the
`project_profile` tool and the `layer` / `by_layer` parameters.

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
emits = `emit:<name>`. TS: a call on a typed parameter / property (`repo: Repo`, `constructor(private repo: Repo)`,
`new Repo()`) resolves to `Repo::find`, so `Type::method` and `via <Interface>` work in TS too; untyped, `any` or
union receivers stay unresolved (search those by name). Searchable keys for `--grep`:

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

## Python

Same switches. File-level functions and constants are in the `<file>.py` `module` container; `class` with bases
(`extends`) and decorators (`uses @dataclass`). Method kinds: `method` / `staticmethod` / `classmethod` / `property`
(`async ` prefix); `self` / `cls` is left out of the signature; a leading `_` = private. Call categories: `native_python`,
`internal_helper`, `static_call`, `instantiation`, `external_call`, `dynamic_call` (`getattr(obj, name)()`, `globals()[name]()`,
`import_module(name)`, `eval` / `exec`). Details: `docs/LANG-PYTHON.md`.

## Go

Same switches. File-level functions and package constants / variables are in the `<file>.go` `module` container;
a `struct` is a `class` (fields = properties, embedded types in `extends`), an `interface` is an `interface` (methods
abstract), `type X int` is a `type`, `type A = B` an `alias`. A method with a receiver (`func (r *Repo) Find`) belongs
to the `Repo` type, even if it is declared in another file. Capitalised name = `public`, lower case = `private`.
`--signatures` shows several return values (`Find(id int): (*User, error)`). Call categories: `native_go`
(builtins, standard library), `internal_helper`, `static_call`, `instantiation`, `external_call`. `_test.go` files
rank as tests in `locate`; `vendor` and `testdata` are skipped. Details: `docs/LANG-GO.md`.

## MCP server (`callgraph mcp`)

<!-- mcp:tools -->
callgraph (`callgraph mcp`): AST-based, token-efficient code map (PHP, Rust, JS / TS / Vue, Swift, Python, Go, SCSS); the CLI's
queries as read-only tools, with the same output. Use it before reading a code file. Paths are relative to the project
root: a registered alias (`project`, list: `projects`) or the server's working directory. Typical flow: `locate`
(where to start) → `outline` → `method_body` for just the method you need → `find_callers` with the name pattern.
- `locate` `{terms}`: where to start with a task (see above): ranked files with reasons, from the names and terms
  of the task in one call; `all: true` = every match, not just the top 5.
- `outline` `{path}`: structure of a file or directory: classes / components, methods with line ranges, calls by
  category. Start here. `method`: one method's call graph; `format`: `api`, `flow` (branches), `summary`, `hotspots`.
- `method_body` `{path, method}`: line-numbered source; `method` is a name or a list (`["store", "update"]`). Compact
  by default (docblock above and blank lines dropped, comments kept, line numbers exact); `doc: true` adds the
  docblock, `raw: true` gives the exact text.
- `find_callers` `{pattern}`: who calls it (file, method, line range, call line); `path` narrows the scope, `depth`
  2–5 = impact analysis (callers of callers), `guess: true` adds name-only matches. JS / TS / Vue keys:
  `useForm.js::submit` (export), `route::orders.show`, `i18n::orders.title`, `vue::emit:save`, `Atoms/AtomButton.vue`.
- `search` `{kind: strings | constants, pattern}`: where a string literal (error message, key) or a constant is.
- `framework_view` `{view, path}`: `routes`, `middleware`, `validation`, `jobs`, `events`, `providers`, `magic`,
  `di`, `views` of a directory.
- `notes`: notes left on files / methods (`📝`, read-only). `guide`: this manual in full (CLI examples, languages).
- `next` `{result_id, choice}`: when the server runs with `callgraph mcp --next`, some results end with
  `↪ r7 possible next: [1] … [2] …` (read-only navigation); run one by number (`choice: 1` or `[1, 3]`), only if it
  fits your task; otherwise ignore the line.
Output is capped at 400 lines (directory → summary, file → method index, search → 1 line per match): narrow `path`,
`method` or `pattern` first; `full: true` only when justified.
<!-- /mcp -->
CLI equivalents: `locate` = `bash $CG locate <term>…`, `outline` = `bash $CG <path>` (`method` = `<path>::name`), `method_body` = `::name --body`,
`find_callers` = `--grep`, `search` = `--strings` / `--const`, `framework_view` = `--format <view>`, `notes` = `--notes`.

## Output limit

Above 400 lines the directory outline switches to `summary`, the single-file outline to a method index, `--grep` to
1 line per match; `--body` / `--lines` / `--strings` / `--depth` stop at 400 lines. Narrow down (`::name`, `--grep`,
subdirectory); `--full` turns the limit off, only when justified. The threshold: `CALLGRAPH_OUT_LIMIT`. A leading `KEY=value` argument is passed on as an environment variable
(e.g. `bash $CG CALLGRAPH_OUT_LIMIT=100 …`).

## Suggested next step (opt-in)

With `CALLGRAPH_NEXT=1` (or `--next`), a result may end with one line such as
`↪ r7 possible next: [1] OrderService.php::refund --body  [2] callers of refund  [3] outline OrderService.php`.
These are read-only navigation steps taken from names in that output, not conclusions. Use one only if it fits your
task (`bash <skill>/callgraph.sh next r7 1`, several: `next r7 1,3`); otherwise ignore the line. Ids are per project
and expire after 24 hours.

## New version notice

If the last line of a `callgraph` output is `ℹ callgraph X.Y.Z is available (installed: …) — update: …` or
`ℹ callgraph X.Y.Z elérhető (telepítve: …) — frissítés: …`, tell the user (once) that a new version is available,
and offer to run `bash <skill>/callgraph.sh update`. Do not update without the user's consent. The line is not
part of the code under inspection; the check runs in the background, at most once a day (opt out: `CALLGRAPH_UPDATE_CHECK=0`).
On a dev build (`X.Y.Z-dev.<commit>`, installed with `update --dev`) the line names both `update` (back to stable) and
`update --dev` (newest dev build); offer the one the user wants. `update --check --dev` only checks for a newer dev build.

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

## Notes (pilot)

The outline and `--body` may show `📝 <time> · <text>` lines under a file or method: notes left by people or agents.
In `--grep` callers, `locate` and `--format api` a trailing `📝 N` marks a hit with notes: read them (outline / `--body`)
before changing it. Take them into account. Leave one when you learn something lasting (why something is so, a pitfall) — short, no task
lists. `note list` marks `[orphaned]` notes (the method is gone; remove with `note rm <id>`).

```bash
bash $CG --project myapp note add app/Order.php::cancel "Refunds run in RefundJob, not here"
bash $CG --project myapp note list app
bash $CG --project myapp app --notes 'refund'
```

## Data and cache

All data goes into the project's own `.callgraph/` directory (cache, project map `index.jsonl`, notes; updated only
for changed files, so repeated searches are fast; the directory is created automatically,
with a `*` `.gitignore`, so it never gets into version control). Nothing is written outside the project. If the directory
is not writable, it runs without a cache. Cleanup: `callgraph prune` (see README).
Local usage stats (counts and ranges only, no paths or names): `bash <skill>/callgraph.sh stats`
when the user asks how callgraph is used; off: `CALLGRAPH_STATS=0`. Sending them is opt-in and only the user decides:
NEVER run `telemetry on` yourself (it asks for a terminal confirmation). If an output ends with a `ℹ for the user:` line
about sharing usage counts, pass it on to the user once and do nothing else.

Processed files: only the supported extensions (upper case too: `B.PHP`), without generated files
(`package-lock.json`, `*.min.js`, `*.min.css`, minified / bundled JS / CSS by content), binary content, the built-in
folders (`vendor`, `node_modules`, …) and what the `.gitignore` files exclude (git rules; every folder's own file). A
`.callgraphignore` file in the project: a line `md` / `.md` / `*.md` skips an extension (the nearest file applies; a
file named directly with such an extension is refused with the reason), any other line is a `.gitignore` pattern
relative to its folder (`public/build`, `/storage/`, `!keep.js`). `--no-gitignore` (any command) or
`CALLGRAPH_GITIGNORE=0` (MCP server, hook) turns the `.gitignore` files off; `.callgraphignore` still applies.

## Limits

<!-- mcp:limits -->
Static analysis: dynamic calls (`$this->$method()`, `obj[key]()`), magic methods and class names / registered components
assembled at runtime cannot be resolved. They are marked `dynamic_call` in the outline (PHP, JS / TS / Vue, Python;
listed last, the target is the called expression): not a caller edge, so `--grep` / `--depth` / `locate` ignore it —
`--depth` only notes how many callers contain one ("check them by hand"). By default `--depth` follows only the certain (resolved) callers;
name-only matches are shown by `--guess` with a "?" mark. For the exact control logic you need `--body` (compact, comments kept; `--raw` gives the exact text).
A `⚠ syntax error` line means a file did not parse fully: the result may be incomplete — read that file directly.
<!-- /mcp -->
The code before and after the broken part is still mapped (JS / TS / Vue, Rust: from the next top-level function,
class or item on); only the broken part is missing. The exit code stays 0; in CI add `--strict` (or set
`CALLGRAPH_STRICT=1`): `show`, `locate` and `profile` then exit with code 3 when a file they read has a syntax error,
with the same output.

`--signatures` (or `CALLGRAPH_SIGNATURES=1`) prints the signature (params, return type) on the outline method lines.
