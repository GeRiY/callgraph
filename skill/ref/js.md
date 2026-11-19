# callgraph — JS / TS

`CG=<skill>/callgraph.sh`, as in [SKILL.md](../SKILL.md) (the core: start there).

## JS / TS

Same switches. TS: a call on a typed parameter / property (`repo: Repo`, `constructor(private repo: Repo)`,
`new Repo()`) resolves to `Repo::find`, so `Type::method` and `via <Interface>` work in TS too; untyped, `any` or
union receivers stay unresolved (search those by name). A function passed as a value (`route('GET', p, rest.listBeds)`,
`auth(handler)`, `[a, b]`, `{ k: handler }`; a local function, a local import or `ns.fn` of a namespace import) is a
reference edge: it appears in callers lists and counts for `--depth`, marked `(ref)`. Searchable keys for `--grep`:

```bash
bash $CG --project myapp resources/js --grep 'src/lib/useForm.js::submit'  # <project-root-relative file>::<export>, with tsconfig paths / Vite aliases
bash $CG --project myapp resources/js --grep 'route::orders.show'     # route usage
bash $CG --project myapp resources/js --grep 'i18n::orders.title'     # where a translation key is used
bash $CG --project myapp resources/js --grep 'api.fetch'             # object-literal method (const api = { fetch() {} })
```

**Route tables** (`--format routes`, any project; on a folder or file): `createRouter({ routes })` / `const routes = [..]`
(vue-router) prints `/chats/:id → client/src/pages/ChatPage.vue (chat)` (nested `children` paths joined, lazy
`import()` resolved, `redirect /x` / `redirect name:x`). Literal names in `router.push({ name })`, `router.replace`,
`<RouterLink :to="{ name }">` that no table defines are flagged `⚠ unknown route name 'x'  file Lnn` (only when a table
exists; `--format ziggy` is the same check without Laravel). Server tables (Hono / Express / itty `app.get('/x', h)`,
`app.route('/p', sub)` / `app.use('/p', router)` mounts, and the `route("GET", /re/, handler)` helper) print
`GET /api/chats → src/api.ts::listChats  ← client/src/composables/api.ts L79`: the handler is resolved through the
file's imports (`ns.fn`, `guarded(fn)` wrappers), an inline function is `file:line`, a regex path is shown as written.
The `←` callers are client `fetch` / `axios.*` / `ofetch` / `new WebSocket` literals (a template literal or `+` part
becomes `:param`; method and whole path must match). A last line `⚠ no server route: GET /api/x (file Lnn)` lists
client URLs under a server route's first path segment that match none (only when server routes are in the queried files).

A TS / JS dictionary module (`export default { … }` or `const en: Schema = { … }` in `locales/en.ts`, `i18n/de.ts`)
is a language file like `lang/en.json`: the `i18n` view reports missing / unused keys across its locales, and its
outline shows the key tree (`bed (3): locked, save, …`). A project's own `t()` wrapper (a local module that imports
`vue-i18n` / `i18next` and exports `t`, `$t`, `tc` or `translate`) makes `t('a.b')` calls `i18n::a.b` keys too.
