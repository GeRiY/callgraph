> Outline JS, TS and Vue single-file components: imports with alias resolution, props, emits, setup, template components and typed-receiver calls.

Source: https://callgraph.eu/typescript/

# TypeScript, JavaScript and Vue call graph

callgraph parses JavaScript, TypeScript and Vue single-file components with [oxc](https://github.com/oxc-project/oxc) and gives your agent modules, functions, classes, component props and emits, template usage and every call by category — plus who uses a function, component, route or translation key across the project.

[Install](https://callgraph.eu/#install) [Use it over MCP](https://callgraph.eu/mcp/)

## What callgraph extracts

Files: `.js` `.mjs` `.cjs` `.jsx` `.ts` `.tsx` `.vue`.

- Modules, functions and classes; TS `interface`s and the `implements` of classes.

- Imports with alias resolution from `jsconfig.json` / `tsconfig.json` (the `@/` alias too).

- Functions inside object literals as their own methods: `const api = { fetch() {} }` → `api.fetch`.

- `if` / `switch` branches, string literals and template literals.

## Vue single-file components

A `.vue` file is a `component`. Its "methods" are what an agent needs to navigate it:

- `setup` — the top-level code of `<script setup>`.

- `template` — the components used, event handlers, `$t()` and `route()` calls.

- Functions, `computed`, `watch@L<n>` / `onMounted@L<n>` hooks, a composable's inner functions (`useX.inner`), and the Options API `methods` / `computed.x`.

- Props are listed as properties, emits as `emit:<name>`.

Calls carry categories such as `vue_api`, `composable_use`, `component_use`, `vue_emit`, `inertia`, `i18n_helper`, `route_helper` (Ziggy), `http_call` (axios), `module_use` and `package_use`.

The `--format api` view lists the component's `props:` (name, type, `[required]`) and `emits:`:

```
component Comp  L1-7

  props (1):
    title: string [required]

  emits (1):
    save
```

## JSX and component use

Plain `.js` / `.mjs` / `.cjs`, `.jsx` and `.tsx` files are mapped together with their JSX, so a React file never ends in a syntax error and an empty outline. (`.ts` stays non-JSX, where `<T>x` is a type assertion.)

A capitalized JSX element (`<Button />`, `<Button>…</Button>`, `<Ui.Button />`) counts as a call of category `component_use`, resolved through the import like an ordinary function call. Lowercase elements such as `<div>` and fragments are not calls. So "who uses this component?" is answered by `--grep` and, for the whole chain, `--depth`:

```
bash ~/.claude/skills/callgraph/callgraph.sh --project web src --grep 'Button$'
```

Output (shortened)

```
FILE  App.jsx

module App  L1-4
  App()  L3-3
    component_use     Button::Button
    L3 │ export function App() { return <div><Button label="x" /></div>; }

---
FILE  Page.tsx

module Page  L1-3
  Page()  L2-2
    component_use     Button::Button
    L2 │ export function Page() { return <section><Button label="a" /></section>; }
```

## Who uses this function, component or key?

The same `--grep` switch understands frontend-specific keys:

```
CG=~/.claude/skills/callgraph/callgraph.sh
bash $CG --project myapp resources/js --grep 'useForm.js::submit'     # <file>::<export>, with import resolution and the @/ alias
bash $CG --project myapp resources/js --grep 'route::orders.show'     # route usage
bash $CG --project myapp resources/js --grep 'i18n::orders.title'     # where a translation key is used
bash $CG --project myapp resources/js --grep 'vue::emit:save'         # who emits 'save'
bash $CG --project myapp resources/js --grep 'Atoms/AtomButton.vue'   # where the component is used
bash $CG --project myapp resources/js --grep 'api.fetch'              # object-literal method
```

`--strings` also finds texts inside Vue template expressions (`{{ … }}`, `:title="…"`, `@click="…"`).

## TypeScript: calls through typed objects

When the type is written down, callgraph works out which class a method call goes to: a typed parameter (`repo: Repo`), a typed class property (`private repo: Repo`, also `constructor(private repo: Repo)`), `this.repo = new Repo()` or `const x = new Repo()`. So `this.repo.find()` shows up as `Repo::find`, and `--grep 'Repo::find'` finds it. Calls made through an interface are marked `via Resolver`.

```
bash $CG --project myapp resources/js --grep 'PlaylistResolver::getPlayables'
```

When the type is unclear — no type, `any`, a union like `Repo | null`, a generic `T` — the call stays unresolved; search those by name: `--grep '(^|::|\.|->)find$'`.

## Limits

- Dynamic calls such as `obj[key]()` and components registered at runtime cannot be resolved statically.

- `defineProps<Props>()` with a referenced type name lists no props (the type is not resolved).

## Related

[

### PHP, Laravel and Yii

The backend side of a Laravel + Vue / Inertia app.

](https://callgraph.eu/php/) [

### Project profile

Recognises a Vue / React frontend and its layers.

](https://callgraph.eu/profile/) [

### locate

Pass a file name like `checkout.vue` among the terms.

](https://callgraph.eu/locate/) [

### MCP server

The same keys work in the `find_callers` tool.

](https://callgraph.eu/mcp/)
