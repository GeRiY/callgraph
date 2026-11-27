---
name: callgraph
description: Use BEFORE your first Grep / Read / Glob in any task that asks which file or function to change, where something is implemented, who calls what, or what a Python / PHP / JS / TS / Vue / Rust / Go / Swift / Kotlin file contains. One `locate` call with the task's names returns the ranked files with reasons; the outline gives a file's methods with line ranges; `--body` prints one method. It reads a prebuilt AST index, so it replaces a Grep + Read chain at a fraction of the tokens. Grep only for exact text; Read only for a line range you already know.
---

# callgraph — code map without Read

`CG=<skill>/callgraph.sh` (`<skill>` = this skill's directory, e.g. `.claude/skills/callgraph` or `~/.claude/skills/callgraph`).
The current directory is the project root; another root: `--project <dir>`.

**First move for any code task:**

```bash
bash $CG locate <names and terms from the task>   # 1. ranked files + reasons (ONE call with all names: functions, classes, files, messages)
bash $CG <file>                                    # 2. outline: classes, methods with line ranges, calls by category
bash $CG <file>::<method> --body                   # 3. one method's source, line-numbered (::save or ::User.save; several: ::a,b)
bash $CG <dir> --grep '(^|::|\.|->)name$'          # 4. who calls it, with the call line; --depth 2 = callers of callers
```

Then answer, or narrow further.

<!-- mcp:start -->
**Use callgraph INSTEAD of Grep / Read / Glob to find and understand code**: it answers from a prebuilt index with a fraction of the tokens. First move: `locate` with every name and term from the task in ONE call → the outline of the top file → `--body` of the one or two methods you must read (never a whole module) → `--grep` with `(^|::|\.|->)name$` for who calls it. Grep only for exact text (a log message, a config value); Read only for a line range you already know.
<!-- /mcp -->

## More queries

```bash
bash $CG locate <terms> --all                 # every matching file, not just the top 5
bash $CG <dir> --strings 'Order not found'    # where a string literal is
bash $CG <dir> --const STATUS_ACTIVE          # a constant: definition and references
bash $CG <dir> --grep 'Order::cancel' --depth 3 --compact   # impact analysis: callers' callers, names + lines only
bash $CG <file> --format flow                 # if / match branches with line ranges
bash $CG <dir> --format summary               # one line per file
bash $CG profile                              # unfamiliar project: architecture, layers, folders (~15 lines)
bash $CG trace route::events.show             # a route / controller action / page: the whole chain in <= 30 lines
bash $CG impact                               # after editing: the changed symbols of the git diff, their callers, routes, pages and tests
```

## Reading the output

<!-- mcp:callers -->
**Callers:** `(^|::|\.|->)name$` catches every call, also with an unknown or inherited receiver (`--depth` 2 = callers of callers); `Type::method` only the calls resolved to that type, so it can miss some: use it to narrow a noisy name (`save`, `get`).
<!-- /mcp -->

<!-- mcp:output -->
**Output:** `root: /abs/path` first line, later paths are relative to it. `L25 │ code` = the call line; `L12-30` = line range; `?::name` / `?` = receiver type unknown, name-only match; `via X` = through an interface / parent. Above 400 lines it becomes a summary: narrow the path, method or pattern (`--full` lifts the cap).
<!-- /mcp -->

- `no call target matches …` / `Did you mean: …` / `No such method`: follow the hint instead of retrying blindly.
- `⚠ syntax error`: that file parsed only partly; read it directly. Dynamic calls (`getattr(obj, name)()`) are not resolved.
- Registering a project alias (`projects.sh add`), all formats, framework views, locate special terms, profile options: [reference.md](ref/reference.md).

## Read next (only what the task needs)

Language notes in this skill's `ref/`: [python.md](ref/python.md), [php.md](ref/php.md), [js.md](ref/js.md), [vue.md](ref/vue.md),
[jsx.md](ref/jsx.md), [rust.md](ref/rust.md), [go.md](ref/go.md), [swift.md](ref/swift.md), [kotlin.md](ref/kotlin.md),
[blade.md](ref/blade.md), [livewire.md](ref/livewire.md), [filament.md](ref/filament.md), [css.md](ref/css.md);
formats, framework views (`--format routes|validation|events|…`), notes, limits: [reference.md](ref/reference.md);
MCP server and its tools: [mcp.md](ref/mcp.md); code generation: [gen.md](ref/gen.md).

MCP clients without file access: `guide {topic: "php"}` with `topic` = `php`, `blade`, `livewire`, `filament`, `js`, `jsx`, `vue`, `css`, `rust`, `swift`, `kotlin`, `python`, `go`, `gen`, `mcp` or `reference` returns the same file.
