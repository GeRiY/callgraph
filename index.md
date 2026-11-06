> Show AI agents a file's structure, call graph and callers without reading it. PHP, JS/TS, Vue, Rust, Swift. Free, MIT, one binary: CLI or MCP server.

Source: https://callgraph.eu/

[v0.5.0 — what's new →](https://callgraph.eu/changelog/)

# Understand code without reading it.

**callgraph** is a token-efficient code map for AI coding agents. It shows the structure, a categorized call graph, single method bodies, *“who calls this?”* and impact analysis — in a few dozen lines instead of whole files.

[Install in one line](https://callgraph.eu/#install) [Download v0.5.0](https://github.com/GeRiY/callgraph/releases/latest) [View on GitHub](https://github.com/GeRiY/callgraph)

```
$ callgraph.sh --project shop app/Models/Order.php
class Order extends Model  L7-23
  constants (1):
    STATUS_OPEN = 'open'
  saveTranslations()  L13-20
    external_call     create, info
    global_helper     logger
    internal_helper   translations
  translations()  L22-22
    orm_relation      hasMany

$ callgraph.sh app --grep saveTranslations --depth 2
== level 1 — directly calls: /saveTranslations/
  app/Models/Order.php  Caller::run()  L25-25
== level 2 — calls the previous level
  (no more callers)
```

## Why agents (and people) like it

Every answer comes from the syntax tree, so it is exact, compact and cheap to read.

### [Where to start with a task](https://callgraph.eu/locate/)

`locate` takes the names and terms from a task in one call and lists the few files worth opening first, best first, each with the reason.

### [Project profile](https://callgraph.eu/profile/)

`profile` tells an agent how an unfamiliar project is built: architecture, which folder holds which layer, design patterns, and the call direction between layers.

### Fewer tokens, faster answers

An outline is a few dozen lines. Your agent reads the map first, then only the exact lines it needs.

### Categorized call graph

Every call is labelled — ORM relation, model use, framework, native, internal helper, SwiftUI view or modifier — so intent is visible at a glance.

### “Who calls this?”

`--grep` finds every caller across the project; `--depth N` follows callers of callers for impact analysis.

### Surgical reads

`file::method --body` prints just that method, line-numbered. `--lines 30-60` prints any range.

### Framework views

Laravel / Yii aware: routes, middleware, validation, jobs, events, providers, DI, views — one command each.

### One static binary, self-updating

No PHP, Node or Rust needed. macOS, Linux and Windows. Fast, cached and incremental; `callgraph.sh update` fetches the newest release and keeps your settings.

### English or Hungarian

All output and help in English by default — switch with `callgraph.sh set-lang hu`.

## When to use it — and when not

callgraph and plain text search do different jobs. Use each for what it is best at.

| The question | Best tool |
|---|---|
| An exact string, log message or config value | grep — plain text search is the best at that |
| Where do I start with this task? | [`locate`](https://callgraph.eu/locate/) — ranked files from the names and terms of the task |
| How is this unfamiliar project built? | [`profile`](https://callgraph.eu/profile/) — architecture, layers, patterns |
| Definitions, structure, callers, impact of a change | callgraph — outline, `--body`, `--grep`, `--depth` |

### When *not* to use it

- You need the exact text of a known line range: use `--lines` or a targeted read.

- Non-code files (plain text, data dumps): the outline adds little.

- Macro- or reflection-heavy code: dynamic calls that can't be resolved statically are shown as name-only matches (`?`) with `--guess`, or not at all.

## Supported languages

One shared output format for every language.

| Language | Files | What you get |
|---|---|---|
| [PHP](https://callgraph.eu/php/) | `.php` | classes, traits, enums, methods, properties, constants, calls with type tracking, Laravel / Yii detection |
| [JavaScript / TypeScript](https://callgraph.eu/typescript/) | `.js .mjs .cjs .jsx .ts .tsx` | modules, functions, classes, imports with alias resolution |
| [Vue](https://callgraph.eu/typescript/) | `.vue` | props, emits, setup, template components and handlers, computed, watch, hooks |
| [Rust](https://callgraph.eu/rust/) | `.rs` | modules, structs, enums, traits, impl blocks, functions, use trees, macro calls, framework detection (axum, actix-web, tokio…) |
| [Swift / SwiftUI](https://callgraph.eu/swift/) | `.swift` | classes, structs, protocols, actors, extensions, View components, property wrappers, SwiftUI views and modifiers |
| CSS / SCSS | `.css .scss` | rule blocks, mixins, functions, variables, imports |
| JSON / Markdown | `.json .md` | key paths; heading sections and links |

## Install

The package contains the Claude Code skill and the binary for your platform. Only `bash` is required.

### macOS / Linux

```
p="$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)" && t="$(mktemp -d)" \
  && curl -fsSL -o "$t/cg.zip" "https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-$p.zip" \
  && mkdir -p ~/.claude/skills && unzip -qo "$t/cg.zip" -d ~/.claude/skills \
  && bash ~/.claude/skills/callgraph/install.sh && rm -rf "$t"
```

### Windows

Download [callgraph-windows-x86_64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-windows-x86_64.zip), unzip it, copy the `callgraph` folder to `%USERPROFILE%\.claude\skills\`, then run in Git Bash:

```
bash ~/.claude/skills/callgraph/install.sh
```

Then register your project once and restart Claude Code:

```
bash ~/.claude/skills/callgraph/projects.sh add shop /absolute/path/to/shop
```

All downloads (macOS Apple Silicon & Intel, Linux x86_64 & ARM64, Windows): [latest release](https://github.com/GeRiY/callgraph/releases/latest).

## Usage

Your agent uses the skill automatically. The same commands work from any terminal.

| Task | Command |
|---|---|
| Where do I start with this task? ([locate](https://callgraph.eu/locate/)) | `callgraph.sh locate <term>... [--all]` |
| How is this project built? ([profile](https://callgraph.eu/profile/)) | `callgraph.sh profile [--full]` |
| Outline of a file or folder | `callgraph.sh <file\|dir>` |
| One method only | `callgraph.sh <file>::<method>` |
| Method source, line-numbered | `callgraph.sh <file>::<method> --body` |
| Where is it called? | `callgraph.sh <dir> --grep <pattern>` |
| Impact analysis | `callgraph.sh <dir> --grep <pattern> --depth 3` |
| String / constant usages | `--strings <pattern>` · `--const <pattern>` |
| Branches with line ranges | `--format flow` |
| Public API only | `--format api` |
| Where to look first | `--format hotspots` |
| Diagrams | `--format mermaid` · `--format dot` |
| Laravel views | `--format routes` · `validation` · `events` · `jobs` |

### Update and language

```
callgraph.sh update --check     # is there a newer release?
callgraph.sh update             # download and install it (settings are kept)
callgraph.sh set-lang hu        # Hungarian output (back: set-lang en)
```

## Use it from any MCP client

callgraph is also an MCP server: `callgraph mcp`. Claude Desktop, Cursor, VS Code, Claude Code or any other MCP client calls the same queries as typed tools, with the same compact output.

### 🔌 Runs locally

Your client starts it on your machine and talks to it over stdin/stdout. No server, and queries never touch the network: your code never leaves the computer. The only network call is a once-a-day update check against GitHub Releases.

### 🔒 Read-only

It never writes your source files, and only reads inside the project folder; paths that point outside are refused.

### 🧰 Ten tools

`locate`, `project_profile`, `outline`, `method_body`, `find_callers`, `search`, `framework_view`, `projects`, `notes`, plus `guide` — the same queries as the commands above, and a short how-to for the agent.

Pick your system — the commands use the binary from the installed skill package. With `--auto-update` (recommended) the server checks once a day at start-up, installs a new release (checksum-verified) and restarts itself; without it, it only tells the agent that an update is available. Turn the check off with `CALLGRAPH_UPDATE_CHECK=0`.

### macOS / Linux

Claude Code (all projects):

```
claude mcp add -s user callgraph -- \
  ~/.claude/skills/callgraph/bin/$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)/callgraph mcp --auto-update
```

For the JSON configs below you need the binary's absolute path — this prints it:

```
echo ~/.claude/skills/callgraph/bin/$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)/callgraph
```

Claude Desktop (`claude_desktop_config.json`) · Cursor (`.cursor/mcp.json`):

```
{
  "mcpServers": {
    "callgraph": {
      "command": "/Users/you/.claude/skills/callgraph/bin/darwin-arm64/callgraph",
      "args": ["mcp", "--auto-update"]
    }
  }
}
```

VS Code (`.vscode/mcp.json`):

```
{
  "servers": {
    "callgraph": {
      "type": "stdio",
      "command": "/Users/you/.claude/skills/callgraph/bin/darwin-arm64/callgraph",
      "args": ["mcp", "--auto-update"],
      "cwd": "${workspaceFolder}"
    }
  }
}
```

Replace the example path with the one printed above (folders: `darwin-arm64`, `darwin-x86_64`, `linux-x86_64`, `linux-aarch64`).

### Windows

Claude Code (PowerShell, all projects):

```
claude mcp add -s user callgraph -- "$env:USERPROFILE\.claude\skills\callgraph\bin\windows-x86_64\callgraph.exe" mcp --auto-update
```

Claude Desktop (`claude_desktop_config.json`) · Cursor (`.cursor/mcp.json`):

```
{
  "mcpServers": {
    "callgraph": {
      "command": "C:\\Users\\you\\.claude\\skills\\callgraph\\bin\\windows-x86_64\\callgraph.exe",
      "args": ["mcp", "--auto-update"]
    }
  }
}
```

VS Code (`.vscode/mcp.json`):

```
{
  "servers": {
    "callgraph": {
      "type": "stdio",
      "command": "C:\\Users\\you\\.claude\\skills\\callgraph\\bin\\windows-x86_64\\callgraph.exe",
      "args": ["mcp", "--auto-update"],
      "cwd": "${workspaceFolder}"
    }
  }
}
```

Replace `you` with your Windows user name (in JSON every `\` is doubled).

Step-by-step setup per client, the ten tools and troubleshooting: [MCP server setup](https://callgraph.eu/mcp/).

The skill and the MCP server work side by side. Clients that don't start in the project folder (e.g. Claude Desktop) use a registered project alias: `projects.sh add shop /path/to/shop`.

## Where to next

Guides for each language and feature.

[

### MCP server setup

Claude Code, Claude Desktop, Cursor and VS Code configs, `--auto-update`, the ten tools.

](https://callgraph.eu/mcp/) [

### PHP, Laravel and Yii

Classes, categorized calls, callers through interfaces and traits, framework views.

](https://callgraph.eu/php/) [

### Rust

Structs, traits, `impl` blocks, macro calls, frameworks from `Cargo.toml`.

](https://callgraph.eu/rust/) [

### JavaScript, TypeScript and Vue

Imports with alias resolution, Vue SFCs, typed receivers, route and i18n keys.

](https://callgraph.eu/typescript/) [

### Swift and SwiftUI

View types as components, property wrappers, SwiftUI views and modifiers.

](https://callgraph.eu/swift/) [

### locate

The few files worth opening first for a task, each with the reason.

](https://callgraph.eu/locate/) [

### Project profile

Architecture, layers and the call direction between them, in about 15 lines.

](https://callgraph.eu/profile/) [

### Changelog

What every release added, changed and fixed.

](https://callgraph.eu/changelog/)

## Bugs and feedback

Found something wrong or have an idea? We read every report. Please check [existing issues](https://github.com/GeRiY/callgraph/issues) first.

### 🐞 Report a bug

Open a [bug report](https://github.com/GeRiY/callgraph/issues/new?template=bug_report.yml). The form guides you through what we need.

### 💡 Ideas and feedback

A missing language or framework, a better output, anything: [share an idea](https://github.com/GeRiY/callgraph/issues/new?template=feature_request.yml).

### 🔒 Security issue

Please **don't** open a public issue — [report it privately](https://github.com/GeRiY/callgraph/security/advisories/new). We answer within a few days.

### What to include in a bug report

- the version: `callgraph --version`

- the exact command you ran and its output

- if the output has an `[error-id: …]` line: the output of `bash ~/.claude/skills/callgraph/feedback.sh show <error-id>` — the error message itself links to the bug form

- a small, non-confidential code snippet that reproduces it, and your OS

## FAQ

### What is callgraph?

A command-line code map for AI coding agents. It parses a file or folder and answers what is in it, which method lives on which lines, what it calls, who calls it and what is affected by a change — in a few dozen lines instead of the whole file.

### Which languages are supported?

PHP (with Laravel and Yii awareness), JavaScript, TypeScript, Vue single-file components, Rust, Swift and SwiftUI, CSS, SCSS, JSON and Markdown. See the [PHP](https://callgraph.eu/php/), [Rust](https://callgraph.eu/rust/), [TypeScript / Vue](https://callgraph.eu/typescript/) and [Swift](https://callgraph.eu/swift/) pages.

### Do I need PHP, Node or Rust installed?

No. callgraph is a single static binary. The only runtime requirement is bash.

### Does it work without Claude Code?

Yes. It ships as a Claude Code skill, but every command works from any terminal or script, and any MCP client (Claude Desktop, Cursor, VS Code…) can use it through `callgraph mcp` — see [MCP setup](https://callgraph.eu/mcp/).

### Is my code sent anywhere?

No. Everything runs locally; your code is never uploaded. The only network call is the update check against GitHub Releases (at most once a day; off with `CALLGRAPH_UPDATE_CHECK=0`).

### How do I update it?

Run `callgraph.sh update`. It downloads the newest release from GitHub, checks it against the release's `SHA256SUMS` and keeps your projects and settings. What changed: [changelog](https://callgraph.eu/changelog/).

### Where do I report a bug or suggest a feature?

On GitHub: [bug report](https://github.com/GeRiY/callgraph/issues/new?template=bug_report.yml) or [idea / feedback](https://github.com/GeRiY/callgraph/issues/new?template=feature_request.yml). Security issues go to a [private advisory](https://github.com/GeRiY/callgraph/security/advisories/new).

### How much does it cost?

Nothing. callgraph is open source under the MIT license.
