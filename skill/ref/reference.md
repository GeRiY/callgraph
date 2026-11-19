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
