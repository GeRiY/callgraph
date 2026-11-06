> callgraph locate takes the names and terms of a task in one call and ranks the few files worth opening first, each with the reason. When to use grep.

Source: https://callgraph.eu/locate/

# locate: where to start with a coding task

Give `locate` the names and domain terms from a task in one call. It answers from the project map, without reading the source, with a short ranked list of the files worth opening first — best first, each with the reason.

[Install](https://callgraph.eu/#install) [The `locate` MCP tool](https://callgraph.eu/mcp/)

## Example

Function, class and file names, column names, routes — all in one go:

```
bash ~/.claude/skills/callgraph/callgraph.sh locate RefundController processRefund refund payment_status checkout.vue --project myapp
```

Output (shortened)

```
locate: 5 terms · 14 matches in 6 files · start here ↓
1. app/Http/Controllers/RefundController.php  ★★★
   class RefundController  L6-16  ← "RefundController", "refund" (exact class)
   store()  L14  calls RefundService::processRefund  ← "processRefund" (call)
2. app/Services/RefundService.php  ★★
   processRefund()  L6-9  callers: 2 (RefundController::store, RefundJob::handle)  ← "processRefund", "refund" (exact method)
…
```

Every file comes with its matching classes or methods, their line ranges and the terms that matched, so the agent can go straight to an [outline](https://callgraph.eu/#usage) or `--body` of the right place.

## How the ranking works

- A match counts more the stronger it is: exact definition, then file name, then a part of a name (`refund` ~ `RefundJob`, `refunds_table`, singular / plural), then a string literal, then a call.

- A file that matches several different terms ranks above one that matches a single term many times.

- 5 files are shown by default; `--all` lists every match.

- Terms without a match and terms that are too general are listed at the end.

**Tip:** pass a non-English domain word together with its English code name (for example a Hungarian visszatérítés together with `refund`).

## locate, grep or callgraph?

| The question | Use |
|---|---|
| An exact string, log message or config value | grep — plain text search is the best at that |
| Where do I start with this task? | `locate` |
| Definitions, structure, callers, impact | callgraph: outline, `--body`, `--grep`, `--depth` |

The typical flow of a task: `locate` → outline of the top files → `--body` of just the method you need → `--grep` to find its callers.

## Over MCP

The `locate` tool takes `terms` (a string or a list), an optional `project` alias and `all: true` for every match. The usage guide the server sends on connect tells the agent to make it the first call of a task. [Set up the MCP server](https://callgraph.eu/mcp/).

## Related

[

### Project profile

In an unfamiliar project, run it once first.

](https://callgraph.eu/profile/) [

### PHP

Outline, callers and Laravel / Yii views.

](https://callgraph.eu/php/) [

### TypeScript / Vue

Routes, i18n keys and components as search keys.

](https://callgraph.eu/typescript/) [

### New in v0.5.0

`locate` arrived in 0.5.0.

](https://callgraph.eu/changelog/#v0.5.0)
