> Run callgraph as a local MCP server: setup for Claude Code, Claude Desktop, Cursor and VS Code, --auto-update, the ten tools and troubleshooting.

Source: https://callgraph.eu/mcp/

# Set up the callgraph MCP server

callgraph is also a [Model Context Protocol](https://modelcontextprotocol.io) server on stdio: `callgraph mcp`. Claude Code, Claude Desktop, Cursor, VS Code or any other MCP client calls the same queries as typed tools — with the same compact text and the same output cap as the command line.

[Pick your client](https://callgraph.eu/mcp/#clients) [The ten tools](https://callgraph.eu/mcp/#tools) [Install callgraph first](https://callgraph.eu/#install)

## Before you start

The MCP server is the same binary the skill uses, so [install the callgraph package](https://callgraph.eu/#install) first. The binary is in the skill package: `~/.claude/skills/callgraph/bin/<os>-<arch>/callgraph` (folders: `darwin-arm64`, `darwin-x86_64`, `linux-x86_64`, `linux-aarch64`; Windows: `bin\windows-x86_64\callgraph.exe`).

JSON configs need the **absolute path** (`~` is not expanded there). This prints it on macOS / Linux:

```
echo ~/.claude/skills/callgraph/bin/$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)/callgraph
```

Not every client starts the server in your project folder, so register your projects as aliases once:

```
bash ~/.claude/skills/callgraph/projects.sh add shop /absolute/path/to/shop
```

## Configure your client

Every example uses `--auto-update` (recommended, see [updates](https://callgraph.eu/mcp/#updates)). Replace `/Users/you/…/darwin-arm64/callgraph` with the path printed above.

### Claude Code

**With the installer:** if the `claude` command is on your PATH, `bash install.sh` asks whether to register the MCP server; `bash install.sh --mcp` does it without asking, `--no-mcp` skips it. Running it again does not create duplicates.

**By hand**, macOS / Linux (picks the binary for your platform; `-s user` makes it available in every project):

```
claude mcp add -s user callgraph -- \
  ~/.claude/skills/callgraph/bin/$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)/callgraph mcp --auto-update
```

Windows (PowerShell):

```
claude mcp add -s user callgraph -- "$env:USERPROFILE\.claude\skills\callgraph\bin\windows-x86_64\callgraph.exe" mcp --auto-update
```

**Check it:** `claude mcp get callgraph` shows the registered command. Restart Claude Code; the callgraph tools are then available. Remove it with `claude mcp remove -s user callgraph`. In Claude Code the skill works too — the two run side by side.

### Claude Desktop

Add this to `claude_desktop_config.json`:

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

Claude Desktop does not start the server in your project folder: name the registered alias (for example `shop`) when you ask, so the tools get the `project` parameter.

**Check it:** restart Claude Desktop and ask it to list callgraph's tools, or to call the `projects` tool — it returns your registered aliases.

### Cursor

Add this to `.cursor/mcp.json`:

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

**Check it:** restart Cursor and ask it to list callgraph's tools.

### VS Code

Add this to `.vscode/mcp.json`:

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

**Check it:** restart VS Code and ask the agent to list callgraph's tools.

### Windows paths in JSON

On Windows the binary is `callgraph.exe` under `bin\windows-x86_64\`. In JSON every `\` is doubled; replace `you` with your Windows user name:

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

For VS Code, use the same `command` value in the `servers` entry above.

## The ten tools

Paths are relative to the project root: a registered alias (`project`) or the server's working directory. A typical flow: `locate` → `outline` → `method_body` → `find_callers`.

| Tool | What it gives | CLI equivalent |
|---|---|---|
| [`locate`](https://callgraph.eu/locate/) | Where to start: ranked files from the names and terms of a task, each with the reason (5 by default, `all: true` for every match). | `locate <term>… [--all]` |
| [`project_profile`](https://callgraph.eu/profile/) | Architecture, layers with folders and file counts, patterns, call direction between layers, unknown folders. | `profile [--full]` |
| `outline` | Structure of a file or folder: classes / components, methods with line ranges, calls by category; `format`: `api`, `flow`, `summary`, `hotspots`, `stats`. | `<path>[::m] --format <f>` |
| `method_body` | Line-numbered source of one or more methods, compact by default; `doc: true` adds the docblock, `raw: true` gives the exact text. | `::m --body [--doc\|--raw]` |
| `find_callers` | Who calls it; `depth` 2–5 for impact analysis, `guess: true` for name-only matches. | `--grep <pattern> --depth N` |
| `search` | Where a string literal or a constant is (`kind`: `strings` \| `constants`). | `--strings` / `--const` |
| `framework_view` | `routes`, `middleware`, `validation`, `jobs`, `events`, `providers`, `magic`, `di`, `views` of a folder. | `--format <view>` |
| `notes` | Notes left on files and methods (read-only; writing notes is CLI only). | `--notes <pattern>` |
| `projects` | The registered aliases and their roots. | `projects.sh list` |
| `guide` | The full skill manual in your language. | `SKILL.md` |

Optional layer parameters (the default output does not change): `layer` on `outline`, `search` and `find_callers` keeps only the files of that layer; `by_layer` on `find_callers` orders callers by layer. See [project profile](https://callgraph.eu/profile/).

On connect the server sends the client a short usage guide in your language (which tool to use when, how to find callers, how to read the output); Claude Code puts it into the agent's instructions. If the server starts in a project folder, a compact project profile is appended.

## What runs locally

### On your machine

Your client starts the server and talks to it over stdin/stdout. Queries never use the network: your code never leaves the computer.

### Read-only

It never writes your source files. Its only side effect is the project map in the project's `.callgraph/` folder, shared with the CLI.

### Confined to the project

Every path resolves against the project root; paths that leave it (`..`, absolute paths outside, symlinks pointing out) are refused.

### Keeps itself fresh

While it runs, it watches the projects it has loaded and refreshes changed files shortly after they are saved. Off with `CALLGRAPH_WATCH=0`.

The only network call is the update check below: at most once a day it looks up the latest GitHub release (and, with `--auto-update`, downloads it).

## Updates: `--auto-update`

- **With `--auto-update` (recommended):** at start, at most once a day, the server checks the latest release. If there is a newer one, it downloads it, verifies it against the release's `SHA256SUMS` (nothing is installed on a mismatch or a missing checksum), installs it exactly like `callgraph.sh update` (your projects and settings are kept) and restarts itself, so the client talks to the new version right away.

- If anything fails (no network, rate limit, bad checksum, no write permission), it writes one line to stderr — `callgraph: auto-update skipped, continuing with …`, which clients log — and starts the current version.

- **Without the flag:** the server checks in the background and adds one line to the first answer of the session: `ℹ callgraph X is available (installed: Y) — update: …`.

- It writes only into callgraph's own folder, never into your projects.

- **Turn all update checks off** with `CALLGRAPH_UPDATE_CHECK=0` in the server's environment (the `env` field of the client config): then there is no network request at all. Checks are also off in CI.

## Troubleshooting

### The client cannot start the server

JSON configs need the absolute path to the binary; `~` is not expanded there. Print the path with the `echo` command from [Before you start](https://callgraph.eu/mcp/#before).

### Paths are not found, or the wrong folder is used

Some clients (for example Claude Desktop) do not start in the project folder. Register the project with `projects.sh add <alias> <path>` and use the alias as the `project` parameter; the `projects` tool lists them.

### A tool returns an error

Errors come back as tool errors in your language; the server keeps running. Paths with `::` are not allowed — the method goes into its own parameter.

### The answer was cut short

Output is capped at 400 lines (a folder outline turns into a summary, a file into a method index). Narrow `path`, `method` or `pattern`; use `full: true` only when needed.

### File watching does not start

If watching is not possible (for example the inotify limit is used up), the server says so once on stderr and checks changed files per request instead.

### I want to test the server by hand

With a graphical inspector: `npx @modelcontextprotocol/inspector <path-to-binary> mcp`.

## Related

[

### Install

The one-line install for macOS / Linux, and Windows.

](https://callgraph.eu/#install) [

### locate

The first call of a task: where to start.

](https://callgraph.eu/locate/) [

### Project profile

How an unfamiliar project is built.

](https://callgraph.eu/profile/) [

### Changelog

What changed in each release.

](https://callgraph.eu/changelog/)
