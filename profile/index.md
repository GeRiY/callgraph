> callgraph profile describes an unfamiliar project in about 15 lines: architecture, layers and folders, patterns, call direction. Plus --layer filters.

Source: https://callgraph.eu/profile/

# Project profile: architecture and layers in about 15 lines

In an unfamiliar project, run `profile` once instead of listing folders. It tells you how the project is built — architecture, which folder holds which layer, design patterns and the call direction between layers — from folder and file names, confirmed by the code graph. It never guesses.

[Install](https://callgraph.eu/#install) [The `project_profile` MCP tool](https://callgraph.eu/mcp/)

## Run it

```
bash ~/.claude/skills/callgraph/callgraph.sh profile --project myapp          # about 15 lines
bash ~/.claude/skills/callgraph/callgraph.sh profile --project myapp --full   # per folder, confidence counts, call pairs between layers
```

## What the profile contains

- **Architecture** — Laravel / Yii layered MVC, DDD / hexagonal, Vue / React frontend, SwiftUI MVVM or Rust web, using the framework found in `composer.json`, `package.json` or `Cargo.toml`.

- **Layers** — which folder holds the controllers, services, repositories, models, components, stores…, with file counts.

- **Design patterns** — Repository, Factory, Strategy, Observer…

- **Call direction** — whether the layers call each other in the expected direction.

- **Unknown folders** — folders without a clear role.

The code graph confirms what the names suggest: `extends Controller`, `implements *RepositoryInterface`, `defineStore`, and the direction of the calls between layers. Every file gets a role with a confidence (high / medium / low), or stays "unknown".

## Use the layers in queries

A layer is a name from the profile (`controller`, `service`, `repository`, `model`, `component`, `store`…), not a path — so you can narrow a query without knowing the folder names. All of this is opt-in; the default output does not change.

```
bash ~/.claude/skills/callgraph/callgraph.sh --project myapp app --layer service                       # only the service files (several: service,repository)
bash ~/.claude/skills/callgraph/callgraph.sh --project myapp app --format summary --layers             # summary grouped by layer
bash ~/.claude/skills/callgraph/callgraph.sh --project myapp app --grep 'Repo::find' --depth 2 --by-layer # callers by layer, closest first
```

- `--layer` is the only one that filters; `--layers` and `--by-layer` only group and order.

- `--by-layer` orders "who calls it?" results by layer, closest to the called code first.

- Over MCP: the `layer` parameter on `outline`, `search` and `find_callers`, and `by_layer` on `find_callers`.

## Your own folder names: `.callgraph-layers`

Folder names that say nothing, or are not English? A commit-able `.callgraph-layers` file in the project root maps them, for example:

```
service = app/Szolgaltatasok, app/Logic
ignore-role = app/Legacy
```

## For AI clients

The `project_profile` MCP tool returns the same profile (`full: true` for the details). When the MCP server starts in a project folder (one with `composer.json`, `package.json`, `Cargo.toml`, `Package.swift`, `.git` or `.callgraph`), a compact profile is sent to the client on connect. [Set up the MCP server](https://callgraph.eu/mcp/).

## Related

[

### locate

Then find the files for the task itself.

](https://callgraph.eu/locate/) [

### PHP, Laravel and Yii

Layered MVC projects.

](https://callgraph.eu/php/) [

### Swift / SwiftUI

SwiftUI MVVM projects.

](https://callgraph.eu/swift/) [

### New in v0.5.0

The project profile arrived in 0.5.0.

](https://callgraph.eu/changelog/#v0.5.0)
