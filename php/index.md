> Outline PHP classes, methods and categorized calls, find every caller and run Laravel / Yii views (routes, validation, events) without reading files.

Source: https://callgraph.eu/php/

# PHP call graph and impact analysis for AI agents

callgraph reads your `.php` files from the syntax tree and tells your agent what is in a class, which method lives on which lines, what each method calls — sorted by category — and who calls it, across the whole project. Laravel and Yii aware, without reading whole files.

[Install](https://callgraph.eu/#install) [Use it over MCP](https://callgraph.eu/mcp/)

## What callgraph extracts from PHP

- Classes, traits and enums, with `extends` and the namespace.

- Methods with exact line ranges, properties, constants and docblocks.

- Calls with type tracking, each labelled with a category.

- String literals and `if` / `match` / `switch` branches (`--format flow`).

- Framework detection: Laravel and Yii.

Call categories you will see in a PHP outline include `native_php`, `orm_relation`, `model_use`, `global_helper`, `internal_helper`, `external_call` and `framework_use`, so the agent sees intent, not just names. Filter by them with `--category model_use,orm_relation`.

## Outline of a model

One command instead of reading the file:

```
bash ~/.claude/skills/callgraph/callgraph.sh --project shop app/Models/Order.php
```

Output

```
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

Then read only the method you need, line-numbered (the docblock above it and blank lines are left out, comments inside stay; `--doc` adds the docblock, `--raw` prints the exact source):

```
bash ~/.claude/skills/callgraph/callgraph.sh --project shop app/Models/Order.php::save --body
```

## Who calls this PHP method?

Start with the name pattern — it catches every call, also where the receiver type is unknown, inherited or comes from a trait. `Type::method` finds only the calls resolved to that type, so use it to narrow a common name like `save` or `find`:

```
bash ~/.claude/skills/callgraph/callgraph.sh --project shop app --grep '(^|::|\.|->)cancel$'
bash ~/.claude/skills/callgraph/callgraph.sh --project shop app --grep 'Order::cancel'
bash ~/.claude/skills/callgraph/callgraph.sh --project shop app --grep 'Order::cancel' --depth 3
```

With `--depth N` callgraph walks callers of callers for impact analysis:

```
$ callgraph.sh --project shop app --grep saveTranslations --depth 2
== level 1 — directly calls: /saveTranslations/
  /srv/shop/app/Models/Order.php  Caller::run()  L25-25

== level 2 — calls the previous level
  (no more callers)
```

- Every match shows the call's own source line, e.g. `L25 │ $this->repo->find($id)`, so the agent sees the arguments without another read.

- Searching `Class::method` also finds calls made through its interface, abstract parent or trait, marked `via Resolver`.

- Calls from child classes that use the inherited method as `$this->method()` are found too, marked with the child class (`L139 via EventeditController │ …`). A child class with its own version of the method is not counted.

- `--guess` adds name-only matches, marked `?`.

## Laravel and Yii views

Framework views run on a directory and give one focused list each: `routes`, `middleware`, `validation`, `jobs`, `events`, `providers`, `magic`, `di` and `views`.

```
bash ~/.claude/skills/callgraph/callgraph.sh --project shop routes --format routes
bash ~/.claude/skills/callgraph/callgraph.sh --project shop app --format validation
```

Over MCP the same views are the `framework_view` tool. In an unfamiliar Laravel or Yii project, [`profile`](https://callgraph.eu/profile/) shows the layered MVC structure first: which folder holds the controllers, services, repositories and models.

## Strings and constants

```
bash ~/.claude/skills/callgraph/callgraph.sh --project shop app --strings 'Order not found'
bash ~/.claude/skills/callgraph/callgraph.sh --project shop app --const STATUS_ACTIVE
```

`--const` lists the definition, the references and re-exports such as `use const App\NAME;`, marked `[re-export]`.

## Limits

This is static analysis: dynamic calls (`$this->$method()`), magic methods and class names assembled at runtime cannot be resolved. By default `--depth` follows only resolved callers; `--guess` shows the name-only ones. For exact control logic, read the method with `--body`.

## Related

[

### locate

Ranked files to start a task with.

](https://callgraph.eu/locate/) [

### Project profile

Layers and patterns of a Laravel / Yii project.

](https://callgraph.eu/profile/) [

### MCP server

The same queries in Claude Desktop, Cursor or VS Code.

](https://callgraph.eu/mcp/) [

### TypeScript / Vue

The frontend side of a Laravel + Vue app.

](https://callgraph.eu/typescript/)
