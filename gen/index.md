> Write PHP, Vue and JS / TS modules, pages and Laravel features from a short expression in your project's style. Rename routes and classes project-wide.

Source: https://callgraph.eu/gen/

# Generator and rename: write and rename code safely

`gen` turns a short Emmet-like expression into real code — PHP classes, Vue components, JS / TS modules, HTML pages, a whole Laravel feature — in the style of your project. `rename` changes a route, class or method name everywhere it is used. Both check syntax before writing and can be undone with one command.

[Install](https://callgraph.eu/#install) [Laravel & Inertia](https://callgraph.eu/laravel/)

## PHP: classes and members

The expression says what the class is, what it extends and implements, and its members: `+` public, `-` private, `#` protected. The namespace comes from the `psr-4` section of `composer.json`, the imports from the project. Names callgraph cannot import are reported as `unresolved: Foo`.

```
bash ~/.claude/skills/callgraph/callgraph.sh gen 'c!:PaymentGateway<BaseGateway>[Chargeable] > ctor(-Client $http, -int $retries=3) + +charge(Order $order):Receipt + -sign(array $p):string' --write app/Services/PaymentGateway.php
```

Without a target, or with `--dry`, it only prints the code. The class above comes out like this:

Generated code

```
<?php

declare(strict_types=1);

namespace App\Services;

final class PaymentGateway extends BaseGateway implements Chargeable
{
    public function __construct(
        private Client $http,
        private int $retries = 3,
    ) {}

    public function charge(Order $order): Receipt
    {
        throw new \LogicException('TODO');
    }

    private function sign(array $p): string
    {
        throw new \LogicException('TODO');
    }
}
```

Add a member to an existing class, after a given method:

```
bash ~/.claude/skills/callgraph/callgraph.sh gen '+refund(Order $o):void{ $this->http->post($o->id); }' --into app/Services/PaymentGateway.php::PaymentGateway --after charge
```

## Vue: components and composables

`vue:Name>(script>…)+(template>…)+(style>…)` writes a single-file component in TypeScript or JavaScript, like the project. In the script: `props(...)`, `emits(...)`, `ref:`, `computed:`, `fn:`, `use:` imports and more. The template uses Emmet-style elements, and components are imported automatically. `comp:useX>…` writes a composable.

```
bash ~/.claude/skills/callgraph/callgraph.sh gen 'vue:Counter>(script>ref:n:int=0+fn:inc(){ n.value++ })+(template>button[@click=inc]{Add})' --dry
```

Generated code

```
<script setup lang="ts">
import { ref } from 'vue'

const n = ref<number>(0)

function inc() {
  n.value++
}
</script>

<template>
  <button @click="inc">Add</button>
</template>
```

Extend an existing page instead of writing a new one:

```
bash ~/.claude/skills/callgraph/callgraph.sh gen 'script>ref:loading:bool=false+fn:reset(){ form.reset() }' --into resources/js/Pages/Auth/Register.vue
```

## Plain JS / TS modules

`mod:` writes a module, `class:` a class. Everything is exported by default; `-fn:` keeps a function private, `!fn:` / `!class:` makes it `export default`. A `.ts` file gets types, a `.js` file JSDoc.

```
bash ~/.claude/skills/callgraph/callgraph.sh gen 'mod:util>fn:join(p:list<str>):str{ return p.join("/") }+const:MAX:int=10' --write resources/js/util.ts
```

Generated code

```
export function join(p: string[]): string {
  return p.join("/")
}

export const MAX: number = 10
```

`--into x.ts[::Class]` adds members to an existing file; `--replace x.ts::fn` or `x.ts::Class::method` replaces one.

## HTML documents

`html:Title[lang=hu]>(head>…)+(body>…)` writes a full document: doctype, charset, viewport and title. The head shortcuts are `meta:description=…`, `link:style.css`, `script:app.js` and `base:/app/`. Any element and attribute works; inline `style{…}`, `script{…}` and `comment{…}` are allowed, and the CSS and JS are checked before writing.

```
bash ~/.claude/skills/callgraph/callgraph.sh gen 'html:Title[lang=hu]>(head>meta:description=Hi+link:style.css+script:app.js)+(body>header>nav>a[href=/]{Home}^^main>form[action=/x method=post]>input[type=email name=e required]+button{Send})' --write index.html
```

`--into x.html` adds at the end of `<body>`, or into `::#id`, `::.class` or `::tag`; `--replace x.html::#id` replaces an element.

## A Laravel feature in one step

The `laravel:` preset writes the FormRequests, controller methods, routes and Inertia / Vue pages together:

```
bash ~/.claude/skills/callgraph/callgraph.sh gen 'laravel:Fee[auth,prefix=admin]>create(name:str,amount:int,paid?:bool)+edit+index'
```

The answer (one line per file)

```
wrote app/Http/Requests/StoreFeeRequest.php (27 lines)
wrote app/Http/Requests/UpdateFeeRequest.php (27 lines)
wrote app/Http/Controllers/FeeController.php (48 lines)
routes/web.php: +5 routes (admin.fees.index, admin.fees.create, admin.fees.store, +2) in auth group
wrote resources/js/Pages/Fees/Index.vue (51 lines)
wrote resources/js/Pages/Fees/Create.vue (45 lines)
wrote resources/js/Pages/Fees/Edit.vue (54 lines)
✓ form ↔ rules (2 forms, 6 fields) · route names (5)
⚠ model Fee not found (app/Models/Fee.php) — no model or migration generated
undo: `callgraph undo` reverts all of these in one step
```

Without Ziggy in the project, the pages use plain URLs (`'/admin/fees'`) instead of `route('…')`.

- It checks that the form fields match the validation rules.

- It warns about NOT NULL columns without a default that the form does not send.

- One `undo` reverts all of its files.

The links it relies on (routes, forms, models, pages) are described on [Laravel & Inertia](https://callgraph.eu/laravel/).

## Where the code goes

- `--write <file>`: a new file only. An existing file is never overwritten.

- `--into <file>[::Class] [--after <method>]`: add to an existing file or class.

- `--replace <file>:L<a>-L<b>` or `--replace <file>::<method>`: replace a line range or one member.

- `--expect <sha256-prefix>` with `--replace`: refuse the change if those lines changed in the meantime.

- `--dry`, or no target at all: only print the code.

The answer is one line per file, so it costs the agent almost nothing to read.

## Safety: check, back up, undo

- The new text is syntax-checked before writing; on an error nothing is written.

- Every change to an existing file is backed up to `.callgraph/backups/` first (the newest 10 per file).

- `undo [<file>]` restores the newest backup; `undo` is itself undoable.

```
bash ~/.claude/skills/callgraph/callgraph.sh undo
bash ~/.claude/skills/callgraph/callgraph.sh undo app/Services/PaymentGateway.php
```

## Rename a route, class or method

`rename route|class|method <old> <new>` changes the name everywhere across PHP, Blade, JS and Vue:

- **route**: `->name()`, groups, `'profile.*'` patterns, resource routes via `->names()`, and every `route('x')` call.

- **class**: the declaration, the file move, namespace, `use` lines, type hints and docblocks. `--strings` also rewrites string references; `--keep-table` keeps the Eloquent table name.

- **method**: overrides, resolved calls, callable arrays and route actions.

```
bash ~/.claude/skills/callgraph/callgraph.sh rename route events.show events.view --dry
bash ~/.claude/skills/callgraph/callgraph.sh rename route events.show events.view
```

Every changed file is syntax-checked first, `--dry` only reports, and the answer is one line per file. Dynamic references (names built at run time) are only listed, never guessed. One `undo` reverts the whole rename.

## Over MCP: opt-in

The MCP `generate` and `rename` tools write files, so they are off by default. They appear in the tool list only when the server runs with `--generate` or `CALLGRAPH_GENERATE=1`; without it the tool list is exactly as before. Unlike the read-only tools, they declare `readOnlyHint: false`.

```
claude mcp add -s user callgraph -- ~/.claude/skills/callgraph/bin/$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)/callgraph mcp --auto-update --generate
```

Setup for the other clients: [MCP server](https://callgraph.eu/mcp/).

## Related

[

### Laravel & Inertia

Routes, forms, models and pages linked in the outline.

](https://callgraph.eu/laravel/) [

### TypeScript / Vue

Outline Vue components and JS / TS modules.

](https://callgraph.eu/typescript/) [

### PHP, Laravel and Yii

Outline classes and find every caller before a rename.

](https://callgraph.eu/php/) [

### MCP server

The opt-in `generate` and `rename` tools.

](https://callgraph.eu/mcp/)
