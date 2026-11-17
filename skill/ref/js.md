# callgraph — JS / TS

`CG=<skill>/callgraph.sh`, as in [SKILL.md](../SKILL.md) (the core: start there).

## JS / TS

Same switches. TS: a call on a typed parameter / property (`repo: Repo`, `constructor(private repo: Repo)`,
`new Repo()`) resolves to `Repo::find`, so `Type::method` and `via <Interface>` work in TS too; untyped, `any` or
union receivers stay unresolved (search those by name). A function passed as a value (`route('GET', p, api.chatSocket)`,
`auth(handler)`, `[a, b]`, `{ k: handler }`; a local function, a local import or `ns.fn` of a namespace import) is a
reference edge: it appears in callers lists and counts for `--depth`, marked `(ref)`. Searchable keys for `--grep`:

```bash
bash $CG --project myapp resources/js --grep 'useForm.js::submit'     # <file>::<export>, with import resolution and the @/ alias
bash $CG --project myapp resources/js --grep 'route::orders.show'     # route usage
bash $CG --project myapp resources/js --grep 'i18n::orders.title'     # where a translation key is used
bash $CG --project myapp resources/js --grep 'api.fetch'             # object-literal method (const api = { fetch() {} })
```
