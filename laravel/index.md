> In Laravel + Inertia (Vue) projects callgraph links route() calls to controllers, checks forms against validation and finds Inertia pages.

Source: https://callgraph.eu/laravel/

# Laravel and Inertia: routes, forms, models and pages linked

In a Laravel + Inertia (Vue) project the interesting facts live in several files: the route table, the controller, the validation rules, the migrations, the Vue page. callgraph reads them for you and puts the short links into the outline of the one file your agent is looking at.

[Install](https://callgraph.eu/#install) [Generator & rename](https://callgraph.eu/gen/)

## Packages: what the project is built from

callgraph reads `composer.json`, `package.json` and the lock files (composer, npm, yarn, pnpm, bun) offline. `profile` then shows a `stack:` line with the notable packages and their locked versions (the declared range when there is no lock, marked as such). Over MCP the startup profile gets a compact version of it.

```
bash ~/.claude/skills/callgraph/callgraph.sh profile --project shop
bash ~/.claude/skills/callgraph/callgraph.sh profile --project shop --packages
```

`--packages` lists every direct composer / npm dependency: installed and declared version, dev flag.

- In an outline, calls into a dependency are grouped under the package's short name, e.g. `guzzle: GuzzleHttp\Client::request`. The JSON output has a `package` field, and `locate` gives a "package" reason.

- `uses <package> [--calls]` (MCP `package_usage`) shows where the code uses a dependency: per API symbol how often and in which file first. `(type)` marks an import without a call; `--calls` lists every call line.

- `deps` (MCP `project_profile` with `deps: true`) lists probably unused, missing and dev-only dependencies, one line each with a short reason. Silence a false alarm with `deps-ignore <name>` in `.callgraphignore`.

```
bash ~/.claude/skills/callgraph/callgraph.sh uses guzzle --calls --project shop
bash ~/.claude/skills/callgraph/callgraph.sh deps --project shop
```

## Routes: from route('x') to the controller

callgraph builds the route table from the loaded `routes/*.php` files: groups, resources, names and the `/api` prefix. In the outline of a Vue or JS file a `route('events.show')` call carries the method, URI and controller action — or `⚠ no route` when the name does not exist. A controller method gets a `route:` line in return.

Outline line (shortened)

```
route::events.update → PUT /events/{event} · EventController@update
```

```
bash ~/.claude/skills/callgraph/callgraph.sh --project shop . --format ziggy
```

The `ziggy` view lists unknown route names (JS, Vue and PHP) and route files that are never loaded. When some routes have dynamic names, unknown names are only marked `?`, not reported as wrong.

## Form check: fields against validation

An Inertia `useForm({...})` form is compared with the validation of the controller action it posts to — a FormRequest or an inline `validate`. The result is a `form:` line under the sending function and under the action:

- fields that are sent but not validated,

- required fields that are not sent,

- or `N fields ✓` when everything matches.

```
bash ~/.claude/skills/callgraph/callgraph.sh --project shop . --format forms
```

The `forms` view lists them all, so a renamed field or a forgotten rule shows up before a user finds it.

## Models: table, columns and relations

Under an Eloquent model the outline shows a `table:` line (the columns from the migrations) and a `fillable:` line. Relation lines show their target model. A `⚠` line warns when a fillable attribute or a cast is not a column.

```
bash ~/.claude/skills/callgraph/callgraph.sh --project shop . --format models
```

The `models` view lists the models, the tables that have no model, and a table that two migrations create without a drop in between — a fresh `migrate` would fail on it.

## Inertia pages: both ends of render()

A PHP `Inertia::render('Board')` call links to its page file (`Pages/Board.vue`). The page's outline gets a `rendered by:` line, so from the Vue file you see which controller sends it.

```
bash ~/.claude/skills/callgraph/callgraph.sh --project shop . --format inertia
```

The `inertia` view lists each page with its render sites and props. It marks a missing page file and props the page does not declare in `defineProps`, and lists the pages that are never rendered.

## Single-file outlines stay fast

The outline of one file reads only the route, migration and form files it needs. The results are kept in `.callgraph/laravel.bin` (only if the `.callgraph/` folder exists), checked against the files' modification time and size, rebuilt automatically and safe to delete. Route files are read in path order, so a controller's `route:` lines come in the same order in every view.

## What this means for your agent

Instead of opening the route file, the controller, the FormRequest and the migration to answer "what does this form post to, and is it validated?", the agent reads the outline of the Vue page or the controller and gets the answer in a few lines. The same views work over MCP (`framework_view` and `project_profile`), see [MCP server](https://callgraph.eu/mcp/).

Want callgraph to write the matching request, controller, routes and pages in one step? See [the Laravel generator](https://callgraph.eu/gen/#laravel).

## Related

[

### PHP, Laravel and Yii

Classes, categorized calls and the framework views.

](https://callgraph.eu/php/) [

### TypeScript / Vue

The frontend side: components, props, emits.

](https://callgraph.eu/typescript/) [

### Generator & rename

Write a Laravel feature in one step, rename a route everywhere.

](https://callgraph.eu/gen/) [

### Project profile

Layers and the `stack:` line of a project.

](https://callgraph.eu/profile/)
