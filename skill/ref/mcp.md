# callgraph — MCP server

`CG=<skill>/callgraph.sh`, as in [SKILL.md](../SKILL.md) (the core: start there).

## MCP server (`callgraph mcp`)

<!-- mcp:tools -->
Tools (paths relative to the project root; `project` = a registered alias, list: `projects`):
- `locate` `{terms}` (`all: true`: every match); `outline` `{path}` (`method`, `format`: `api` / `flow` / `summary`); `method_body` `{path, method}` (a name or a list; `doc: true`, `raw: true`); `find_callers` `{pattern}` (`depth` 2–5, `guess: true`).
- Also: `search` `{kind: strings | constants, pattern}`, `framework_view` `{view, path}`, `trace`, `impact` `{diff?, staged?}`, `notes`, `next`, `guide` (`topic`: language / reference files).
<!-- /mcp -->
Tool details (the long form of the list above):

callgraph (`callgraph mcp`): AST-based, token-efficient code map (PHP / Blade, Rust, JS / TS / Vue, Swift, Kotlin, Python, Go, SCSS); the CLI's
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
  `useForm.js::submit` (export), `route::orders.show`, `i18n::orders.title`, `Card.vue::emit:save`, `Atoms/AtomButton.vue`.
- `search` `{kind: strings | constants, pattern}`: where a string literal (error message, key) or a constant is.
- `framework_view` `{view, path}`: `routes`, `middleware`, `validation`, `jobs`, `events`, `providers`, `magic`,
  `di`, `views` of a directory; Laravel + Inertia (project root): `ziggy`, `forms`, `models`, `inertia`; any project:
  `i18n` (translation keys ↔ `$t()` / `__()`), `classes` (CSS classes ↔ templates).
- `impact` `{diff?, staged?, depth?}`: what the current change affects: the git diff (working tree vs HEAD; `staged: true`; `diff`: `main...HEAD`, `HEAD~3`) as changed symbols with their callers (`depth` 1–3, `?` = name-only), routes / pages / FormRequest rules of a changed controller method, and the tests that call them. Call it after editing.
- `notes`: notes left on files / methods (`📝`, read-only). `guide`: the core manual (CLI examples); `topic` (`php`, `blade`, `js`, `vue`, `css`, `rust`, `swift`, `kotlin`, `python`, `go`, `gen`, `mcp`, `reference`) returns one reference file.
- `next` `{result_id, choice}`: when the server runs with `callgraph mcp --next`, some results end with
  `↪ r7 possible next: [1] … [2] …` (read-only navigation); run one by number (`choice: 1` or `[1, 3]`), only if it
  fits your task; otherwise ignore the line.
Output is capped at 400 lines (directory → summary, file → method index, search → 1 line per match): narrow `path`,
`method` or `pattern` first; `full: true` only when justified.

CLI equivalents: `locate` = `bash $CG locate <term>…`, `outline` = `bash $CG <path>` (`method` = `<path>::name`), `method_body` = `::name --body`,
`find_callers` = `--grep`, `search` = `--strings` / `--const`, `framework_view` = `--format <view>`, `trace` = `bash $CG trace <target>`, `impact` = `bash $CG impact`, `notes` = `--notes`.

## Wiring clients (`callgraph setup`)

`callgraph setup [--client claude,cursor,codex,vscode,windsurf] [--project <dir> | --user] [--dry-run] [--uninstall] [--yes]`
writes, per client, the MCP entry (`<this binary> mcp`) and a short use-callgraph-first rule; no `--client` = the clients
found. Default scope: the project (cwd, or `--project <dir>`); `--user` = user-level files (Codex and Windsurf MCP config is
always user-level). Files: Claude Code `.mcp.json` + `.claude/rules/callgraph.md`; Cursor `.cursor/mcp.json` +
`.cursor/rules/callgraph.mdc`; VS Code `.vscode/mcp.json` + `.github/copilot-instructions.md`; Codex `~/.codex/config.toml` +
`AGENTS.md`; Windsurf `~/.codeium/windsurf/mcp_config.json` + `AGENTS.md`. `AGENTS.md` / instruction files get a
`<!-- callgraph:start -->` … `<!-- callgraph:end -->` block (only that block is touched). Existing config is merged, changed
files get a `.bak`, a second run changes nothing, `--dry-run` writes nothing (prints files and snippets), `--uninstall` removes
only what setup wrote. Restart the client afterwards.
