# callgraph — PHP / Laravel / Inertia

`CG=<skill>/callgraph.sh`, as in [SKILL.md](../SKILL.md) (the core: start there).

## PHP / Laravel

PHP receiver types come from `new Foo()`, `Foo::create()`, typed parameters / properties and the Laravel container:
`$x = app(Foo::class)` (also `resolve()`, `app()->make()`, `$this->app->make()`, `App::make()`) and the chained
`app(Foo::class)->bar()` resolve to `Foo::bar`; a string id (`app('foo')`) does not.
`Foo::class`, `[Foo::class, 'm']` (resolves to `Foo::m`) and `Route::get($uri, Foo::class)` (`Foo::__invoke`) are
reference edges marked `(ref)`: `--grep Foo` and `locate Foo` also find the routes / config / bindings that name it.

**Framework views** (`--format …`): `routes`, `middleware`, `validation`, `jobs`, `events`, `providers`,
`magic`, `di`, `views`. Run them on a directory (e.g. `bash $CG --project myapp routes --format routes`). Laravel +
Inertia, on the project root: `ziggy` (unknown route names), `forms` (useForm ↔ validation rules), `models`
(tables and columns from the migrations), `inertia` (pages, render sites, props). In any project, on a folder or a
file: `i18n` (translation keys missing per locale, unused keys, `$t('a.b')` / `__('x')` calls whose key does not
exist; `lang/en.json::board.title` shows where one key is used) and `classes` (stylesheet classes and the templates
using them, plus classes nothing uses). Lang files and stylesheets also get the summary line in their outline.

Laravel wiring (no call exists in the code, the framework makes it): `Shelf::query()->stocked()` resolves to
`Shelf::scopeStocked` (`--grep 'Shelf::scopeStocked'` lists the callers); an event's dispatch site is a caller of its
listener (`handle(Event $e)` in a `Listeners` namespace, `Event::listen(E::class, L::class)`, the `$listen` array), a
job's `X::dispatch` of `X::handle`, `->notify(new N)` of `N::toMail`, and `Model::observe(O::class)` / `#[ObservedBy]`
of the observer's `created` / `updated` … methods. `--format events` shows the pairs (`wired  Event  dispatch: …  →
listener: …`), `jobs` the `queued` classes and their dispatchers.
`--format routes` on a Laravel project is the full route table (one source for `ziggy`, `inertia`, `forms`): verb, full
URI, handler, `name=`, `[middleware]`, `file:line`; a controller group's string actions (`->controller(X::class)->group(..)`
→ `'index'`), prefix, name and middleware are inherited through nesting, and routes generated from a literal config
array (`foreach (config('site.pages') as $name => $page) Route::get($page['uri'], $page['action'])->name($name)`) are
expanded (a `component` key marks the Inertia page as rendered). A `config/*.php` or `lang/*/*.php` file (`return [ … ]`)
outlines as key paths (`pages.home.uri = '/'`).
