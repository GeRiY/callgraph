<h1 align="center">callgraph</h1>

<p align="center">
  <b>A token-efficient code map for AI coding agents.</b><br>
  Structure, categorized call graph, method bodies, "who calls this?" and impact analysis —
  without reading whole files.<br>
  🌐 <a href="https://callgraph.eu/">callgraph.eu</a>
</p>

<p align="center">
  <a href="https://github.com/GeRiY/callgraph/actions/workflows/release.yml"><img alt="build" src="https://github.com/GeRiY/callgraph/actions/workflows/release.yml/badge.svg?branch=main"></a>
  <a href="https://github.com/GeRiY/callgraph/releases/latest"><img alt="release" src="https://img.shields.io/github/v/release/GeRiY/callgraph?display_name=tag&sort=semver"></a>
  <a href="LICENSE"><img alt="license: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <img alt="platforms" src="https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey">
</p>

<p align="center">
  <a href="#quick-start">Quick start</a> ·
  <a href="#installation">Installation</a> ·
  <a href="#usage">Usage</a> ·
  <a href="#how-it-works">How it works</a> ·
  <a href="#development">Development</a> ·
  <a href="README.hu.md">Magyar</a>
</p>

---

An agent that needs to understand one method usually reads the whole file, then greps, then reads a few more files.
**callgraph** answers those questions from the AST instead, in a few dozen lines: what is in this file, which method
lives on which lines, what it calls (sorted by category), who calls it, and what breaks if it changes.

It ships as a single native Rust binary plus a thin [Claude Code](https://claude.com/claude-code) skill, and it works
just as well from any terminal or script.

```console
$ callgraph.sh --project shop app/Models/Order.php
FILE  /srv/shop/app/Models/Order.php  NS:App\Models

class Order extends Illuminate\Database\Eloquent\Model  L7-23
  constants (1):
    STATUS_OPEN = 'open'
  properties (1):
    mixed $fillable
  saveTranslations()  L13-20
    external_call     create, info
    global_helper     logger
    internal_helper   translations
  translations()  L22-22
    orm_relation      hasMany
```

```console
$ callgraph.sh --project shop app --grep saveTranslations --depth 2
== level 1 — directly calls: /saveTranslations/
  /srv/shop/app/Models/Order.php  Caller::run()  L25-25

== level 2 — calls the previous level
  (no more callers)
```

## Highlights

- **Reads the AST, not the text.** Classes / components, methods with exact line ranges, properties, constants,
  imports, docblocks, string literals and `if` / `match` / `switch` branches.
- **Categorized call graph.** Every call is classified: `native_php`, `orm_relation`, `model_use`, `global_helper`,
  `internal_helper`, `external_call`, `framework_use`, Vue `composable_use` and more, so the agent sees intent,
  not just names.
- **"Who calls this?" and impact analysis.** `--grep <pattern> --depth N` walks callers of callers across the whole
  project using a cached, incremental project index.
- **Surgical reads.** `file::method --body` prints just that method, line-numbered; `--lines 30-60` prints any range.
- **Framework views.** Laravel / Yii aware: `routes`, `middleware`, `validation`, `jobs`, `events`, `providers`,
  `magic`, `di`, `views`.
- **Many output formats.** `outline`, `api`, `flow`, `summary`, `hotspots`, `coverage`, `stats`, `mermaid`, `dot`,
  `json`, `jsonl`.
- **Fast and dependency-free.** One static binary (no PHP, Node or jq needed), parallel parsing, mtime-based JSONL cache.
- **English or Hungarian output**, switchable with one command.

### Supported languages

| Language | Extensions | What you get |
|---|---|---|
| PHP | `.php` | classes, traits, enums, methods, properties, constants, calls with type tracking, framework detection (Laravel, Yii) |
| Rust | `.rs` | modules, structs, enums, traits, `impl` blocks, functions with visibility / `async`, fields, consts, `use` trees, path / method / macro calls, `if` / `match` flow, framework detection from `Cargo.toml` (axum, actix-web, tokio, serde, …) |
| JavaScript / TypeScript | `.js` `.mjs` `.cjs` `.jsx` `.ts` `.tsx` | modules, functions, classes, imports with alias resolution (`jsconfig` / `tsconfig`) |
| Vue | `.vue` | props, emits, `setup`, template components and event handlers, `computed`, `watch`, lifecycle hooks, Options API |
| Swift / SwiftUI | `.swift` | classes, structs, enums, protocols, actors, extensions; `View` types as components; `func` / `init` / computed properties (`body`); property wrappers (`@State`, `@Binding`, `@Published`, …); SwiftUI views and modifiers as their own call categories |
| CSS / SCSS | `.css` `.scss` | rule blocks, mixins, functions, variables, `@use` / `@import` |
| JSON / Markdown | `.json` `.md` | key paths; heading sections and links |

### Which files are processed

Only files with the extensions above, in upper or lower case (`B.PHP` counts as PHP); everything else is ignored.
Also skipped, even with a supported extension: generated files (`package-lock.json`, `*.min.js`, `*.min.css`) and
files with binary content. Each file is read once, even if a symlink points to it. Folders like `vendor/`,
`node_modules/` and those listed in `.gitignore` are skipped too.

To exclude more extensions in a project, put a `.callgraphignore` file in its root (or in any subfolder; the nearest
one applies). Commit it, and the whole team gets the same results:

```
# callgraph: skip these extensions in this project
md
json
*.vue
```

### When *not* to use it

- You need the exact text of a known line range: use `--lines` or a targeted read.
- Non-code files (plain text, data dumps): the outline adds little.
- Macro- or reflection-heavy code: dynamic calls that can't be resolved statically are shown as name-only matches
  (`?`) with `--guess`, or not at all.

## Quick start

```bash
# 1. install (macOS / Linux, see below for Windows)
p="$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)" && t="$(mktemp -d)" \
  && curl -fsSL -o "$t/cg.zip" "https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-$p.zip" \
  && mkdir -p ~/.claude/skills && unzip -qo "$t/cg.zip" -d ~/.claude/skills \
  && bash ~/.claude/skills/callgraph/install.sh && rm -rf "$t"

# 2. register a project once
bash ~/.claude/skills/callgraph/projects.sh add shop /absolute/path/to/shop

# 3. ask questions
CG=~/.claude/skills/callgraph/callgraph.sh
bash $CG --project shop app/Models/Order.php                      # outline
bash $CG --project shop app/Models/Order.php::save --body         # one method's source
bash $CG --project shop app --grep 'Order::cancel' --depth 3      # impact analysis
```

Restart Claude Code afterwards so it picks up the skill (`SKILL.md`).

## Installation

Prebuilt packages (skill files + the binary for your platform + install guide) are attached to every
[GitHub Release](https://github.com/GeRiY/callgraph/releases/latest):

| Platform | Package |
|---|---|
| macOS, Apple Silicon (M1–M4) | [callgraph-darwin-arm64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-darwin-arm64.zip) |
| macOS, Intel | [callgraph-darwin-x86_64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-darwin-x86_64.zip) |
| Linux x86_64 (static musl) | [callgraph-linux-x86_64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-linux-x86_64.zip) |
| Linux ARM64 (static musl) | [callgraph-linux-aarch64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-linux-aarch64.zip) |
| Windows x86_64 (Git Bash) | [callgraph-windows-x86_64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-windows-x86_64.zip) |

All five binaries in one archive: `callgraph-<version>.tar.gz` on the same release page.

**macOS / Linux:** use the one-liner from [Quick start](#quick-start). It installs into `~/.claude/skills/callgraph/`
and runs `install.sh`. Updating is the same command; your `projects.json`, `errors.jsonl`, `feedback.jsonl` and
language setting are kept.

**Windows / manual:** unzip the package, copy the `callgraph/` folder under `~/.claude/skills/`
(Windows: `%USERPROFILE%\.claude\skills\`) and run `bash install.sh` there (Git Bash). On macOS `install.sh` also
clears the download quarantine flag. Step-by-step guide (Hungarian): [`skill/TELEPITES.md`](skill/TELEPITES.md).

**From source:** see [Development](#development).

The only runtime requirement is `bash`.

### Updating

```bash
bash ~/.claude/skills/callgraph/callgraph.sh update --check   # is there a newer release? (changes nothing)
bash ~/.claude/skills/callgraph/callgraph.sh update           # download and install it
```

The update downloads your platform's zip from the latest GitHub Release and overwrites the skill files and `bin/`;
`projects.json`, `errors.jsonl`, `feedback.jsonl` and the language setting are kept. `--force` reinstalls the current
version; `callgraph --version` shows the installed one.
Every release ships a `SHA256SUMS` file; the update verifies the downloaded zip against it and installs nothing on a
mismatch. Releases without it (v0.1.0) are refused unless you pass `--insecure-skip-checksum`. With `--from <zip>` the
zip is checked against `--sha256 <hex>` or a `SHA256SUMS` next to it (otherwise a one-line warning).
`callgraph.sh` also checks for a new release in the background at most once a day (without slowing queries down) and,
if there is one, ends its output with a one-line `ℹ callgraph X is available` notice; `CALLGRAPH_UPDATE_CHECK=0`
turns this off (it is also off when `CI` is set).

## Usage

All commands go through `callgraph.sh` (or the binary directly: `callgraph show …`). `--project <alias>` makes paths
relative to a registered project root; without it, the current directory is the root.

| Task | Command |
|---|---|
| Where do I start with this task? | `callgraph.sh locate <term>... [--all]` |
| Outline of a file / directory | `callgraph.sh <file\|dir>` |
| One method only | `callgraph.sh <file>::<method>` (several selectors allowed) |
| Method source, line-numbered | `callgraph.sh <file>::<method> --body` |
| Any line range | `callgraph.sh <file> --lines 30-60` |
| Where is it called? | `callgraph.sh <dir> --grep <pattern>` |
| Impact analysis | `callgraph.sh <dir> --grep <pattern> --depth N [--guess]` |
| String literal / constant usages | `callgraph.sh <dir> --strings <pattern>` · `--const <pattern>` |
| Filter by call category | `callgraph.sh <dir> --category model_use,orm_relation` |

### Where to start: `locate`

Give `locate` the names and domain terms from the task in one call (function, class and file names, column names,
routes); it answers from the project map, without reading the source, with a short ranked list of files to start
from, each with the reason:

```bash
bash ~/.claude/skills/callgraph/callgraph.sh locate RefundController processRefund refund payment_status checkout.vue --project myapp
```

```
locate: 5 terms · 14 matches in 6 files · start here ↓
1. app/Http/Controllers/RefundController.php  ★★★
   class RefundController  L6-16  ← "RefundController", "refund" (exact class)
   store()  L14  calls RefundService::processRefund  ← "processRefund" (call)
2. app/Services/RefundService.php  ★★
   processRefund()  L6-9  callers: 2 (RefundController::store, RefundJob::handle)  ← "processRefund", "refund" (exact method)
…
```

A match counts more the stronger it is: exact definition, then file name, then a part of a name (`refund` ~
`RefundJob`, `refunds_table`, singular / plural), then a string literal, then a call. A file that matches several
different terms ranks above one that matches a single term many times. By default 5 files are shown (`--all`: every
match); terms without a match and terms that are too general are listed at the end. Plain text search (an exact
message or config value) is still a job for grep; `locate` answers "where do I start?".

### Output formats (`--format`)

| Format | Gives you |
|---|---|
| `outline` (default) | structure + calls by category |
| `api` | public contract + docblocks, no implementation |
| `flow` | `if` / `match` / `switch` branches with line ranges (condition → what it calls) |
| `summary` | one line per file |
| `hotspots` | top 20 methods by in/out degree |
| `coverage` | undocumented public methods |
| `stats` | category statistics |
| `mermaid` / `dot` | diagrams of inheritance and call edges |
| `json` / `jsonl` | the raw graph |
| `routes` `middleware` `validation` `jobs` `events` `providers` `magic` `di` `views` | framework views (run on a directory) |

Large results are capped (above 400 lines a directory outline turns into a summary, a single file into a method
index); `--full` lifts the cap. Full reference: `callgraph.sh --help` and [`skill/SKILL.md`](skill/SKILL.md).

### Language

Terminal output and help are in English by default; Hungarian is available:

```bash
bash ~/.claude/skills/callgraph/callgraph.sh set-lang hu    # save (back: set-lang en)
bash ~/.claude/skills/callgraph/callgraph.sh set-lang       # show current language and its source
CALLGRAPH_LANG=hu bash ~/.claude/skills/callgraph/callgraph.sh app/Models/Order.php   # one run only
```

### Use with other AI tools (MCP)

Besides the Claude Code skill (unchanged, fully supported), the same binary is also an
[MCP](https://modelcontextprotocol.io) server on stdio, so any MCP client can use it: `callgraph mcp`.
Tools: `locate`, `outline`, `method_body`, `find_callers`, `search`, `framework_view`, `notes`, `projects`, `guide`. They return
the same compact text as the CLI, with the same output cap; read-only, paths are confined to the project root, and
queries never use the network (only the once-a-day update check below does).
On connect the server sends the client a short usage guide in your language (which tool to use when, how to find
callers, how to read the output); Claude Code puts it into the agent's instructions. The `guide` tool returns the full
skill manual. Both come from the same source as `SKILL.md`, so they always match.

The installer can register it for Claude Code for you: `bash install.sh` asks (if the `claude` command is available),
`bash install.sh --mcp` does it without asking, `--no-mcp` skips it. Remove it with
`claude mcp remove -s user callgraph`. By hand:

```bash
# Claude Code, macOS / Linux (picks the binary for your platform):
claude mcp add -s user callgraph -- \
  ~/.claude/skills/callgraph/bin/$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)/callgraph mcp --auto-update
```

```powershell
# Claude Code, Windows (PowerShell):
claude mcp add -s user callgraph -- "$env:USERPROFILE\.claude\skills\callgraph\bin\windows-x86_64\callgraph.exe" mcp --auto-update
```

Claude Desktop, Cursor and VS Code use a JSON entry with `"command": "<absolute path to the binary>", "args": ["mcp", "--auto-update"]`.
**Recommended: `--auto-update`** — at start (at most once a day) the server installs a newer release exactly like
`callgraph.sh update` (SHA-256 checked, your settings kept) and restarts itself, so the client always talks to the
latest version; if anything fails it just starts the current one. Without the flag it only adds a one-line
"newer version available" note to the first answer of the session. It writes only into callgraph's own folder; opt
out of all update checks with `CALLGRAPH_UPDATE_CHECK=0`.
Recipes, tool parameters and safety notes (Hungarian): [`docs/MCP.md`](docs/MCP.md).
While it runs, the MCP server watches the projects it has loaded and refreshes changed files by itself.

### Keeping the project map fresh

After a `git pull` or many edits, `bash ~/.claude/skills/callgraph/callgraph.sh refresh [--project alias]` (add
`--background` to return at once) updates the project map in advance, so the next query does not have to re-analyse.
Optional Claude Code hook: `bash install.sh --hooks` (or `callgraph.sh hook install`) refreshes every file Claude
edits in the background; it adds one entry to `~/.claude/settings.json`, keeps everything else and writes a
`settings.json.bak` backup first. Remove it with `bash ~/.claude/skills/callgraph/callgraph.sh hook uninstall`.
Details (Hungarian): [`docs/PROJECT-GRAPH.md`](docs/PROJECT-GRAPH.md).

### Feedback from agents

Every error message carries an `[error-id: …]`. Agents (or you) can attach a note to it, which makes broken
cases easy to collect and fix:

```bash
bash ~/.claude/skills/callgraph/feedback.sh add <error-id> "what I expected, what happened"
bash ~/.claude/skills/callgraph/feedback.sh list --open
```

## How it works

```
files ─► FileWalker ─► LanguageRegistry ─► Language::analyze ─► FileGraph ─► JSONL cache / project index ─► query formats
         (.gitignore,    (by extension)     (mago · oxc · raffia ·   (one shared
          vendor, …)                         pulldown-cmark · syn ·    schema)
                                             tree-sitter-swift)
```

- Every language is a module under `src/lang/<language>/` that maps its AST into one shared `FileGraph` schema; the
  query layer (`src/query/`) never needs to know which language a file was.
- Parsers: [mago](https://github.com/carthage-software/mago) (PHP), [oxc](https://github.com/oxc-project/oxc)
  (JS / TS / Vue), [raffia](https://github.com/g-plane/raffia) (CSS / SCSS),
  [pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark) (Markdown), [syn](https://github.com/dtolnay/syn) (Rust), [tree-sitter-swift](https://github.com/alex-pinkus/tree-sitter-swift) (Swift).
- Results are cached per file in JSONL (keyed by mtime); "who calls this?" uses an incremental project index (mtime + size) under
  `.callgraph/`.

Design and coding rules: [`ARCHITECTURE.md`](ARCHITECTURE.md) · query layer: [`docs/QUERY.md`](docs/QUERY.md) ·
per-language mapping: [`docs/LANG-JS.md`](docs/LANG-JS.md), [`docs/LANG-CSS.md`](docs/LANG-CSS.md),
[`docs/LANG-DOC.md`](docs/LANG-DOC.md), [`docs/LANG-RUST.md`](docs/LANG-RUST.md), [`docs/LANG-SWIFT.md`](docs/LANG-SWIFT.md) (the detailed docs are currently in Hungarian).

## Development

Only Docker is needed locally; the toolchain (Rust stable, rustfmt, clippy, musl target, python3, php) lives in the
dev image (`docker/dev.Dockerfile`):

```bash
bash tools/dev.sh check     # fmt --check + clippy -D warnings + tests (debug profile, like CI) — run before every commit
bash tools/dev.sh build     # release binary: ~/.cache/callgraph-dev/target/release/callgraph
bash tools/dev.sh test      # tests only
bash tools/dev.sh skill     # dist/callgraph-<version>.tar.gz (skill package, Linux)
bash tools/dev.sh shell     # shell inside the container
```

The build output is shared by all checkouts / worktrees (`CALLGRAPH_DEV_TARGET`, default
`~/.cache/callgraph-dev/target`; `CALLGRAPH_DEV_TARGET=local` keeps a per-checkout `target/docker/`), so a new branch
doesn't rebuild every dependency. `check-release` runs the same checks with the release profile (LTO, slow).
With a local toolchain the usual `cargo build --release` / `cargo test` work too. CI
([`.github/workflows/release.yml`](.github/workflows/release.yml)) builds and tests macOS (arm64, x86_64),
Linux (arm64, x86_64, static musl) and Windows on every push, and publishes a GitHub Release on `v*` tags.

### Bugs and feedback

Found a bug or have an idea? [Open an issue](https://github.com/GeRiY/callgraph/issues/new/choose) — the bug form asks for `callgraph --version`, the command and the output (with its `[error-id: …]` line).

### Contributing

Issues and pull requests are welcome. Before opening a PR, run `bash tools/dev.sh check`.
Adding a language is one new module plus one registration line: see [`docs/UJ-NYELV.md`](docs/UJ-NYELV.md).
Refactors must keep the output byte-identical (`tools/identity_check.sh`).

### Security

Please report vulnerabilities privately through
[GitHub Security Advisories](https://github.com/GeRiY/callgraph/security/advisories/new), not in public issues — see [`SECURITY.md`](SECURITY.md).

## Author

Created and maintained by **Gergő Makra** — [LinkedIn](https://www.linkedin.com/in/gergomakra/).

## License

[MIT](LICENSE)
