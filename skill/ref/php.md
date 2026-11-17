# callgraph — PHP / Laravel / Inertia

`CG=<skill>/callgraph.sh`, as in [SKILL.md](../SKILL.md) (the core: start there).

## PHP / Laravel

PHP receiver types come from `new Foo()`, `Foo::create()`, typed parameters / properties and the Laravel container:
`$x = app(Foo::class)` (also `resolve()`, `app()->make()`, `$this->app->make()`, `App::make()`) and the chained
`app(Foo::class)->bar()` resolve to `Foo::bar`; a string id (`app('foo')`) does not.

**Framework views** (`--format …`): `routes`, `middleware`, `validation`, `jobs`, `events`, `providers`,
`magic`, `di`, `views`. Run them on a directory (e.g. `bash $CG --project myapp routes --format routes`). Laravel +
Inertia, on the project root: `ziggy` (unknown route names), `forms` (useForm ↔ validation rules), `models`
(tables and columns from the migrations), `inertia` (pages, render sites, props). In any project, on a folder or a
file: `i18n` (translation keys missing per locale, unused keys, `$t('a.b')` / `__('x')` calls whose key does not
exist; `lang/en.json::board.title` shows where one key is used) and `classes` (stylesheet classes and the templates
using them, plus classes nothing uses). Lang files and stylesheets also get the summary line in their outline.
