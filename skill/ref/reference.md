# callgraph — reference (rarely needed)

`CG=<skill>/callgraph.sh`, as in [SKILL.md](../SKILL.md) (the core: start there).

## Project profile — details

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

## Suggested next step (opt-in)

With `CALLGRAPH_NEXT=1` (or `--next`), a result may end with one line such as
`↪ r7 possible next: [1] OrderService.php::refund --body  [2] callers of refund  [3] outline OrderService.php`.
These are read-only navigation steps taken from names in that output, not conclusions. Use one only if it fits your
task (`bash <skill>/callgraph.sh next r7 1`, several: `next r7 1,3`); otherwise ignore the line. Ids are per project
and expire after 24 hours. `CALLGRAPH_NEXT_LEARN=1` (extra opt-in, off by default) only re-orders or drops these
suggestions from your own local log; nothing leaves the machine. `next --eval` checks that log offline (CLI only).

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

## Install flags: optional hint hook

`bash <skill>/install.sh --nudge-hook` adds a non-blocking `PreToolUse` hook (matcher `Grep|Read|Bash`, command
`callgraph hook pre-nudge`) to `.claude/settings.json` (skill in `<project>/.claude/skills`) or `~/.claude/settings.json`
(user install). Default off (`--no-nudge-hook` skips the prompt); `--remove-nudge-hook` or `callgraph hook uninstall-nudge` removes
only this entry (`callgraph hook uninstall` removes all callgraph hooks). Existing settings and hooks stay, `settings.json.bak`
is written first, no duplicates. When the agent greps (Grep, or Bash `grep` / `rg`) for a code identifier or reads a
whole source file (4 KB or more, no offset / limit) in a project with a callgraph index, the hook adds one line
(`additionalContext`) pointing to `locate` / the outline; once per pattern per session; never blocks, always exits 0.

## Limits

<!-- mcp:limits -->
Dynamic calls (`obj[key]()`) are not resolved; `--guess` adds name-only matches. `⚠ syntax error`: the file parsed only partly, read it directly.
<!-- /mcp -->
Details: dynamic calls and magic methods are marked `dynamic_call` in the outline (PHP, JS / TS / Vue, Python;
listed last, the target is the called expression); they are not caller edges, so `--grep` / `--depth` / `locate` ignore
them — `--depth` only notes how many callers contain one ("check them by hand"). For the exact control logic you need
`--body` (compact, comments kept; `--raw` gives the exact text).
The code before and after the broken part is still mapped (JS / TS / Vue, Rust: from the next top-level function,
class or item on); only the broken part is missing. The exit code stays 0; in CI add `--strict` (or set
`CALLGRAPH_STRICT=1`): `show`, `locate` and `profile` then exit with code 3 when a file they read has a syntax error,
with the same output.

`--signatures` (or `CALLGRAPH_SIGNATURES=1`) prints the signature (params, return type) on the outline method lines.


## Registering a project (`projects.sh`)

The current directory is the project root. To use short paths from anywhere, register the project once:

```bash
bash <skill>/projects.sh add myapp /absolute/path/to/the/project/root
bash <skill>/projects.sh list
```

After that, with `--project myapp` you can give short paths relative to the root; `--project <dir>` (an existing
folder, absolute or relative) works without registering.

## `locate` — special terms and examples

```bash
bash $CG locate RefundController processRefund refund payment_status checkout.vue --project myapp   # ranked files + reasons
bash $CG locate refund --all --project myapp                                                       # every matching file, not just the top 5
bash $CG locate route::events.show "Szervező további eseményei" --project myapp                     # a route (definition first) and a UI phrase
```

Give `locate` the names and domain terms of the task in ONE call: function, class, property and file names, columns,
routes, translation keys, and for a non-English domain word its English code name too (e.g. visszatérítés → `refund`;
a plain sentence works too, filler words are ignored). It answers from the project map without reading files: the top 5
files (`--all`: every match), each with its reasons (exact definition, file name, name part, string, call, `callers: N`,
`+M?` = name-only guesses).

A quoted phrase with spaces (`"Szervező további eseményei"`) is ONE term, matched as a whole against string literals (UI
labels, translations), never split into words; `route::events.show` finds the route definition (`->name('events.show')`
in `routes/*.php`, ranked first) and its `route()` / `to_route()` / `redirect()->route()` calls in PHP and JS / TS / Vue;
`--orange-500` is a CSS custom property, not a flag. Files with an exact definition or file name rank before files that
only match strings (translations, build output). A name that joins several terms (`toggleFavorite` for `toggle favorite`,
`PodcastEpisodeController` for `podcast episode`) ranks high even when each term alone is too general; definitions in type
declarations (`.d.ts`) weigh less than real ones.

## Recipes — task → call sequence

An empty or wrong result comes with a hint (`no call target matches …`, `Did you mean: …`): follow it instead of retrying blindly.

- **Fix a bug / change behaviour:** `locate <the names in the task>` → the outline of the top file (`callgraph.sh <file>`) →
  `callgraph.sh <file>::<method> --body` for just the method you need.
- **Work on a route / page (Laravel, Inertia, Vue, JS server routes):** `callgraph.sh trace route::events.show` first (also
  `Ctrl::method`, a URI, `Pages/X.vue`): the whole chain in <= 30 lines (route, middleware, action, rules, models, page + props).
- **"Who calls this / what does it affect if I change it":** `callgraph.sh <dir> --grep '(^|::|\.|->)name$' --depth 2`
  (the name pattern catches every call) → `callgraph.sh <caller file>::<caller method> --body`.
- **Unfamiliar project:** `callgraph.sh profile` (architecture, layers) → `locate <terms>` → the outline of the files it lists → `--body`.
- **"Where is this text / constant":** `--strings <fragment>` (UI text, messages; a case-insensitive substring) or `--const <name>`;
  for an exact text, or in plain-text files, use Grep.

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
bash $CG --project myapp app --grep cancel                          # a plain word = the whole name (cancel, x.cancel, T::cancel; not cancelAll); '.*cancel.*' = substring
bash $CG --project myapp app --grep '(^|::|\.|->)cancel$'           # who calls it: EVERY call of the name, with the call line
bash $CG --project myapp app --grep 'Order::cancel'                  # only calls resolved to Order (+ via interface / parent / trait)
bash $CG --project myapp app --grep 'Order::cancel' --depth 3        # impact analysis: the callers' callers
bash $CG --project myapp app --grep 'Order::cancel' --depth 3 --compact # impact as file-grouped names + lines, no code
bash $CG --project myapp app --grep 'Order::cancel' --depth 3 --guess # + name-only matches ("?")
bash $CG --project myapp app --grep 'Order::cancel' --body           # source of the caller methods found
bash $CG --project myapp app --grep 'App\Svc\Order::cancel'        # PHP namespace: a single \ is read as the separator (App\\Svc\\ also works); a bad regex gets a 💡 hint
bash $CG --project myapp app --category model_use,orm_relation       # filter by call category
bash $CG --project myapp app --category dynamic_call                 # the calls whose target is decided at runtime
```

`🔒` after a file path: the file is protected; read it, don't edit it. `L12, L40 │ …`: the same call on several lines
(at most 3 rows per caller, then `… +2 more: L55, L61`). The rest of the output legend: [SKILL.md](../SKILL.md).

## Project profile — commands

```bash
bash $CG profile --project myapp                                     # architecture, layers (folders, file counts), patterns, call direction, unknown folders
bash $CG profile --project myapp --full                              # per folder, confidence counts, the call pairs between layers
bash $CG --project myapp app --layer service                         # only the files of one layer (several: service,repository)
bash $CG --project myapp . --layer controller:js                     # one ecosystem only (php, js, swift, rust) when a role is in both
bash $CG --project myapp app --format summary --layers               # summary grouped by layer
bash $CG --project myapp app --grep 'Repo::find' --depth 2 --by-layer # callers grouped by layer, a "[service]  3 callers" header per group
```

`--layers` / `--by-layer` only group and order, never hide results. Details: "Project profile — details" above.

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

Framework views (`--format routes|middleware|validation|jobs|events|providers|magic|di|views`, Laravel + Inertia
`ziggy|forms|models|inertia`, any project `i18n`, `classes`): [php.md](php.md). A `📝 <time> · <text>` line (or a
trailing `📝 N`) is a note left by a person or an agent: read it before changing that code (see Notes above).

## Output limit

Above 400 lines the directory outline switches to `summary`, the single-file outline to a method index, `--grep` to
1 line per match; `--body` / `--lines` / `--strings` / `--depth` stop at 400 lines. Narrow down (`::name`, `--grep`,
subdirectory); `--full` turns the limit off, only when justified. The threshold: `CALLGRAPH_OUT_LIMIT`. A leading
`KEY=value` argument is passed on as an environment variable (e.g. `bash $CG CALLGRAPH_OUT_LIMIT=100 …`).
