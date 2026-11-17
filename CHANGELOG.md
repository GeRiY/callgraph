# Changelog

All notable changes to callgraph. Work is collected in packages on the `develop` branch and released together.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versions: [SemVer](https://semver.org/).

## [Unreleased]

### Added

- **Blade templates (`.blade.php`) are analysed as their own language** (before, the result was empty): one view per
  file, named like `admin.users.index`, with sections, pushes and slots as line ranges; `@extends` / `@include` /
  `@each` / `<x-…>` / `<livewire:…>` resolve to the view files, so the callers of a layout list the views extending
  it; `@props` show as properties; PHP inside `{{ }}`, `{!! !!}`, `@php` and directive arguments is analysed;
  `@if` / `@foreach` … appear as control flow; `route('x')` links to the Laravel route
  (`→ POST /users · Controller@method`, unknown names are flagged); `__()` / `@lang` take part in the i18n check; and
  a "rendered by" line lists the `view('…')` calls in controllers.
- **MCP `undo` tool:** restores the last change made by `generate` or `rename` — one file when a file is given,
  otherwise the whole last group of changes. The history is multi-step: each call steps one change further back
  (before, two undos in a row toggled the last change). Like `generate` and `rename`, it only exists when the server runs with
  `--generate` / `CALLGRAPH_GENERATE=1`, and it is marked `readOnlyHint: false`.
- **`gen --dry` shows where the code would go:** with `--into` / `--replace`, the dry run first prints the placement,
  e.g. `→ app/Pay.php::Pay after charge (L41)` or `→ f.vue: script L12-14`.
- **`gen --diff` and more placement options:** `--diff` shows a `--replace` as a short unified diff without writing
  anything. For PHP `--into`, `--before <method>` and `--at L<n>` place the code before a method or at a line, and
  `--upsert` turns an `--into` of an existing method into a replace. Vue `--into file.vue::template#id` (or `::#id`,
  `::.class`) appends inside that template element. Components named in `.callgraph/gen.toml` are also found behind
  tsconfig / Vite aliases. The MCP `generate` tool got `before`, `at`, `upsert` and `diff`.
- **Vue props from interfaces, defaults and `defineModel`:** `defineProps<Props>()` now resolves a local `interface`
  or type alias (including `extends` and `A & B`); `withDefaults` defaults are shown as `type $name = default` and the
  prop is not marked required; `defineModel` appears as `model:` in the outline and as a `model` section in
  `--format api`. A type-only `defineEmits` now shows its real line.
- **Kotlin support (`.kt`, `.kts`):** classes, data/sealed/enum classes, objects and companion objects, interfaces,
  top-level and extension functions, `suspend` and `@Composable` functions, properties (including `by lazy` and
  `by viewModels()`), calls with receiver type resolution, `::ref` / `Type::name` references and control flow.
  Categories cover Compose (views, modifiers, API), coroutines, Android and the Kotlin standard library;
  `*.gradle.kts` files are marked as Gradle build scripts. `hiltViewModel()`, `viewModel()` and `by viewModels()` are
  injection edges, and calls on the view model resolve to its type. Hilt `@Inject constructor`, `@Provides` and
  `@Binds` show what is injected and which method provides a type. `NavHost` `composable("route") { Screen() }` and
  `navController.navigate("…")` are navigation edges (type-safe routes too, `navigate(Detail(…))` / `composable<Detail>`), also when the `NavHost`
  is in another file. Callers of an interface method and of
  its implementations see each other. Android projects get a profile (ui / view model / data / domain / di layers;
  `*.gradle.kts` is not counted as app code), and Kotlin appears in the usage statistics. `!!` does not hide the
  receiver of a call, `*Test.kt` files and `androidTest` folders count as tests in `locate`, and method bodies are
  filtered correctly (comments, raw strings).
- **Swift callbacks, protocols and navigation:** a method reference passed as a callback argument
  (`Composer(onSend: vm.send)`) is a call of that method. A closure stored in a callback property
  (`socket.onFrame = { … }`) is linked to where the property is invoked; with `--depth 2` the invoking method shows
  as a caller `via <Type>::onFrame`. Callers of a protocol method and of its implementations see each other (`via`),
  and a conformance declared in an `extension` counts. `.environment(store)` / `.environmentObject` are injection
  edges, and views built in `navigationDestination`, `sheet`, `fullScreenCover`, `popover` or `NavigationLink` are
  navigation edges. Receiver types are taken from `@State`, `@Bindable`, `let x: T`, `@Environment(T.self)` and
  parameters. A reference passed positionally (`.onChange(of: x, vm.changed)`) or via `#selector(…)` is a reference
  call; key paths are not calls. SwiftData `@Query var items: [Item]` is a model edge and `modelContext` calls are
  categorized. Each view in a `navigationDestination { switch … }` is a navigation edge, generic views
  (`List<Item>(…)`) count as component uses, and actor methods resolve. A call through a property chain that spans
  files (`self.store.session.user.load()`) resolves to the method of the last property's type. The method that
  invokes a stored closure (`onFrame?(…)`) is listed as a direct caller of the closure's callees, under "called
  through a stored closure", as `via <Type>::<property>`. Callers resolved across files (Kotlin routes, Swift property
  chains, Rust trait bounds) are shown at the first level of the callers list, under "called through a stored closure
  or resolved across files".
- **`callgraph mcp --allow <dir>`:** lets the MCP server read an extra folder, e.g. a sibling `../android`. Repeat
  the flag or set `CALLGRAPH_MCP_ALLOW`; by default the server stays restricted to the project root.
- **Rust: function references and more receiver types:** a function or method passed as a value (`.map(Self::parse)`,
  `.and_then(Foo::new)`, `.map(helper)`) is a call. Receivers typed by a generic bound (`T: Trait`, `where`) or by a
  method's return type (`self.make()?`, `Type::open().unwrap()`) are resolved. Loop variables (`for b in self.bars`),
  `if let Some(x)` / `while let` / `match Some(x)` bindings and iterator-closure parameters
  (`items.iter().map(|x| x.name())`) get their element type, so their method calls resolve; with `T: A + B` the trait
  that declares the method is used, also when the traits are declared in other files. `#[test]` / `#[tokio::test]` functions and `#[cfg(test)]` modules are marked as
  tests, `Some` / `Ok` / `Err` / `Box::new` / `Default::default` no longer clutter the call list, and calls of a local
  closure are marked dynamic. Callers of a trait method and of its implementations see each other in both directions
  (`via`).
- **JS/TS: functions passed as values are callers:** a function handed over as a call or `new` argument, an array
  element or an object property value (also shorthand), e.g. the handler in a route table `guarded(api.listX)`, is a
  reference edge marked `(ref)`. It shows in callers lists, counts for `--depth` / impact, and the function no longer
  looks like it has no callers. This covers functions of the same file, local imports, object-literal methods and
  members of a namespace import (`import * as api`); package functions are not counted.
- **More JS/TS imports resolve to files:** tsconfig `extends` (a string or an array, also chained) and `references`
  are followed, a `paths` entry may have several targets and a `*` in the middle (the longest matching prefix wins),
  and `resolve.alias` in `vite.config.*` / `vitest.config.*` is read (an object or `[{find, replacement}]`, with
  `fileURLToPath(new URL(…))` or `path.resolve(__dirname, …)`). An alias is used only if its target exists. So imports
  like `@shared/api` or `@kit/…` find their callers and `deps` no longer reports them as missing packages. Imports
  without an extension also find `.tsx`, `.jsx`, `.mts`, `.cjs`, `.cts` files and `index.vue` / `index.tsx` /
  `index.jsx` / `index.mjs`, and `./a.js` in a TypeScript project resolves to `a.ts`. Import resolution is faster
  too: each folder is listed once instead of checking every candidate file.
- **PHP: `X::class` references:** `X::class`, a callable pair `[X::class, 'method']` (`X::method`) and an invokable
  controller in `Route::get($uri, X::class)` (`X::__invoke`) are reference edges marked `(ref)`, resolved through
  `use` and the namespace. `--grep X`, `locate X` and `--depth` now also find the routes, config entries and bindings
  that name the class.
- **Rust CLI and library profile:** a crate with `main.rs`, `lib.rs` or `bin` targets gets a profile with its entry
  points, `src/<module>` folders as module layers, and workspace members listed. `tests`, `benches`, `examples` and
  fixture or golden-sample folders no longer make the project look like a web, Vue or SwiftUI project.
- **TypeScript interface members as properties:** the members of an `interface` are listed as properties, optional
  ones with `?`.
- **Broken links in Markdown and HTML are marked:** a relative link to a missing file, or a `#anchor` with no
  matching heading, shows `⚠ missing`.
- **Locked versions in `package.json` / `composer.json`:** the outline shows the locked version next to each
  dependency key.
- **Translation keys with `--format i18n`:** for a lang file it shows which keys are missing in the other locales
  and which are unused; for a key (`lang/en.json::board.title`) where it is used and its value per locale; for source
  code which used keys do not exist (`⚠ missing in hu.json`). Covers lang JSON and PHP files and `$t` / `t` / `trans` /
  `__` / `@lang` / `Lang::get` usages. Dynamic keys (`` `x.${y}` ``, concatenation) are never reported missing, and
  sentence-style keys (Laravel's "strings as keys") are never reported unused.
- **Stylesheet class usage with `--format classes`:** shows which templates and components use a class, and a lenient
  list of unused classes (dynamic class building and Tailwind utilities are never reported). Lang files, stylesheets
  and `$t(…)` calls also get a short hint in the outline, and the MCP `framework_view` tool got `i18n` and `classes`.
- **MCP `guide` tool with a `topic`:** the optional `topic` parameter (e.g. `{topic: "php"}`) returns one detail file
  of the skill instead of the core manual, so clients that only use MCP can read them too. Topics: `php`, `blade`,
  `js`, `vue`, `css`, `rust`, `swift`, `kotlin`, `python`, `go`, `gen`, `mcp`, `reference`.

### Changed

- **JS/TS/Vue file keys are relative to the project root:** the file part of a key (`ui/src/x.ts::fn`) is relative to
  the nearest folder with `.git` or `.callgraph` (without one, the topmost folder of the package / workspace), with `/`
  on every platform. In a monorepo a file has one key, so `--grep 'ui/src/x.ts::fn'` finds its callers in every
  package; before, each importing package saw its own relative key. Single-package projects keep their keys. The
  stored graph is rebuilt once, quietly.
- **Shorter skill, details per language:** the agent skill is now a short English core `SKILL.md` (about 13 KB
  instead of 30 KB, roughly 3 500 instead of 7 800 tokens) plus detail files in `ref/` (php, blade, js, vue, css,
  rust, swift, kotlin, python, go, gen, mcp, reference) that the agent reads only for the languages of the project.
  Kotlin and Blade are covered in the skill for the first time. `self-update` installs the new layout.
- **Less noise in the framework views:** `--format events`, `middleware`, `validation`, `jobs`, `providers` and `di`
  match their patterns only at the right place, so e.g. `config` or `response` no longer show up as events, while
  `X::dispatch` and `Event::listen` are listed. The middleware label is no longer glued to its target, and validation
  only counts strings that are real rule lists. References are not calls in these views, so
  `belongsTo(Event::class)` is not listed as an event dispatch.
- **Full translation keys in the i18n view:** `--format i18n` shows whole keys with the shared prefix written once
  (`a.b.{c, d}`) instead of cut-off keys, and it is faster in projects without translation files.
- **CSS/SCSS `var()` shows the property:** a `var(--brand)` call is listed as `var(--brand)` and is found by `--grep`
  both as `var` and as `--brand`. Functions such as `translateY`, `blur` and `scale` are no longer hidden.
- **Cleaner rows in the outline and views:** enum and constant reads in PHP show as `const_use` instead of
  `external_call`; `--format routes` rows no longer end in `@ __top__::__top__`, and Vue method index rows no longer
  carry `[private]`.
- **Single-file output uses a relative path:** when a `show` output contains the project path only once, the `FILE`
  path is relative and there is no `root:` line, so it can be copied into the next call.
- **Stories count as tests:** story files (`*.story.*`, `*.stories.*`) and `stories` / `storybook` folders are tests
  in the project profile, not pages.
- **The Laravel form check follows `form.transform(...)`:** `--format forms`, the `form:` lines and the form ↔ rules
  check of the `laravel:` generator now check the keys the transform returns; with a `...data` spread, the form
  fields plus the added keys. A form submitted with a chained `form.transform(…).post(…)` is found too.
- **The Laravel preset picks form components more carefully:** a component only gets a role if it has the prop that is
  bound (an error-state block is no longer used as the field error). A field wrapper component (label + error + slot,
  e.g. `FormField`) wraps each field, and textarea and checkbox components are used for `text` / `bool` fields; the
  `.callgraph/gen.toml` keys are `field`, `textarea` and `checkbox`. The preset also finds form components behind a
  tsconfig `paths` / Vite alias (e.g. `@ui` pointing to a separate UI package), preferring components that existing
  pages already import.
- **Clearer marks in the output (all languages):** reference calls are marked `(ref)` in the outline, `--grep` rows
  and callers. Test functions get `[test]`; a module of only tests gets `[tests]` and is collapsed to a count line.
  Injection, navigation and callback-wiring edges show their kind (`inject`, `nav`, `wire`) in the category column.
- **`gen` is faster in large PHP projects:** unknown type names are resolved with a single project walk (before: one
  walk per unresolved name), each project file is read at most once per call, and the Laravel preset reads
  `composer.json` once.
- **Shorter MCP usage errors:** a usage error of `generate`, `rename` or `undo` over MCP no longer appends the whole
  usage text; the CLI is unchanged. The `generate` tool description is shorter too — the grammar is in the guide and
  in the skill's `ref/gen.md` (MCP: `guide` with `topic: "gen"`).
- **The `laravel:` preset writes through the `gen` generator:** FormRequests, controller methods and Inertia/Vue pages
  now follow the same formatting as `gen` itself: PHPDoc `@return array<string, list<string>|string>`, Vue pages with
  `function submit()`, long tags wrapped one attribute per line, the `@inertiajs/vue3` import first, and
  `const props =` only where the script uses it. Ziggy is detected from the `composer.json` / `package.json`
  dependency lists instead of a text search.
- **Stylesheet calls and JSON keys in the outline:** CSS/SCSS calls (`var()`, `@include`, functions, `@extend`) appear
  under their rule and are found by `--grep`. A JSON file without methods gets a short key index in the method index
  (top keys with child counts, runs of scalars collapsed). `--format summary` counts keys, sections or rules instead
  of "0 methods" for JSON, Markdown/HTML and stylesheets.
- **More accurate SCSS outline:** `&--#{$x}` and `.span-#{$i}` keep their full selector name; multi-line selectors and
  mixin parameter lists are read correctly; `@mixin` without parentheses and `@include x;` (a call) work; calls in
  comments and selector headers are no longer reported; a reassigned `$var` in the same scope is a write, not a second
  constant; trailing `//` comments are recorded. CSS and SCSS now share one engine.
- **Markdown links merged:** the outline lists the same link target once, with `×N`.
- **No repeated tip over MCP:** the closing 💡 tip line is no longer added to every MCP answer (the CLI keeps it).
  The skill now also recommends callgraph for the structure of md, json, css, scss and html files.
- **Multi-file `--format summary` shows the folder:** rows of the same folder are grouped under a `folder/` header
  line; a lone file shows its relative path.
- **Swift project profile:** folders such as Features, Core, Shared, Networking, UI and Persistence get layers; an
  `@Observable` / `ObservableObject` class is a store candidate, `View` conformers are views, and `Codable` /
  `Identifiable` types are models. More Swift and Foundation built-ins are categorized (`pow`, `defer`, math
  functions, `os_log`, UIKit and Foundation types).

### Fixed

- **Literal `import('…')` is a use:** a dynamic `import('./x')` or `import('pkg')` with a fixed string in JS, TS or
  Vue now links to the file or package, so `deps` no longer reports such packages as unused and the importing
  component lists the files it loads.
- **`deps`: test environment packages are tooling:** `jsdom`, `happy-dom`, `jest-environment-*` and `@vitest/*` are
  no longer reported as unused.
- **Vue: methods on `computed(…).value` are not Vue API calls:** with `const x = computed(…)` or `ref(…)`, a call such
  as `x.value.filter(…)` is a plain call on the data, no longer reported as a Vue API call.
- **`--grep` shows the matching line of a chained call:** for a call chain spread over several lines, the shown
  source line is the line of the match, not the first line of the chain.
- **An unknown `--option` is an error:** a mistyped option of an analysis now stops with the usage text instead of
  printing raw JSONL output.
- **The `laravel:` preset syntax-checks generated `.ts`, `.js` and `.html` files before writing:** before, only PHP
  and Vue files were checked.
- **Module paths of JS/TS files outside a project:** a file with no `package.json`, `tsconfig` or `jsconfig` above it
  got module paths like `home/…`; the path is now relative to the file's folder.
- **Rust: the crate's own modules are own code:** `mod x;` and inline `mod` paths no longer count as a package.
- **`--grep <Component>.vue --depth N` follows template usage at every level:** it was empty from level 2
  (also on Windows).
- **Laravel routes registered in a loop:** a route with an interpolated name (`->name("{$code}.x")`) makes
  `route('en.x')` "dynamic, not verifiable" instead of unknown, and a helper outside `routes/` that registers such
  routes is pointed out. The same applies to groups with a computed prefix or name
  (`Route::prefix($code)->name($code.'.')->group(…)`, config-driven loops) in `--format ziggy`.
- **Typed one-line `defineEmits` is read:** `defineEmits<{ login: []; save: [id: number] }>()` on one line now
  produces the emits list (`--format api`).
- **No false calls from template-literal text in Vue attributes:** text inside a template literal of a bound
  attribute (`` :title="`… név (pl. …)`" ``) is no longer read as a call (accented letters were the trigger).
- **`--format forms` keeps forms submitted to a computed URL:** `form.post(path.value)` is listed, and its route is
  marked unknown/dynamic.
- **`--grep useX --depth N` skips destructured values:** uses of a value destructured from `useX()` are no longer
  listed as calls of `useX`.
- **`callgraph.sh rename` works:** the wrapper script now forwards it (the help already advertised it).
- **`gen --at L<n>` outside the class body is rejected upfront:** e.g. on the `{` line; the message gives the body's
  line range.
- **`gen … --into file.vue::#id` / `::.class` reads template markup:** the expression is parsed as markup by default.
- **`gen --into … --upsert --expect …` is accepted:** the expectation is checked when the member already exists.
- **The `laravel:` preset follows the project and the migration:** it writes `<script setup lang="ts">` with typed
  props in a TypeScript project even without existing `.vue` files, and reads the migration column type: `text`
  columns get no `max:255`, `string('x', 100)` gets `max:100`.
- **`locate` caller counts stay within the language family:** a Vue function no longer counts the PHP callers of a
  same-named method.
- **Swift: an `Extensions` folder is own code:** a folder named `Extensions` with Swift files was treated as vendor.

### Removed

- **Hungarian skill variant and `skill-sync`:** the skill and the MCP guide are English only; the CLI output language
  is unchanged (`callgraph set-lang hu`, `CALLGRAPH_LANG`). `callgraph skill-sync` is removed, `set-lang` no longer
  rewrites `SKILL.md`, and `self-update` deletes the old `skill-i18n/` folder.

## [1.0.2] — 2026-11-16

### Changed

- **The `root:` line shows the git branch:** the `root:` line at the top of multi-file outputs (and `locate`) now ends
  with the branch of the project root, e.g. `root: /x/app (branch: feature/z)`; on a detached HEAD it shows
  `detached <sha7>`, and without git it is unchanged. An agent sees at once when it looks at another checkout than the
  one where the branch was switched (a `--project` alias pointing to another clone or worktree, an MCP server started
  in another folder).

### Fixed

- **Short `::method` selector works in a multi-selector call:** `callgraph.sh src/Sample.php::alpha ::beta --body`
  now shows both bodies; `::beta` reuses the file of the previous selector. Before, it answered "Not found: <root>/".
  A `::method` with no file before it gives a clear usage error, and the help lists the form.

## [1.0.1] — 2026-11-16

### Fixed

- **Laravel route files loaded from a group keep the group's prefixes:** a file loaded with `->group(base_path('routes/admin.php'))`,
  `->group(__DIR__.'/admin.php')`, a `require` inside a group, or a group in the `then:` closure of `bootstrap/app.php`
  now inherits the group's URI prefix and route-name prefix. Existing chained names like `admin.reports.show` are no
  longer reported as unknown by `--format ziggy` (false alarms), and `rename route` checks names the same way.
- **Unnamed routes in a named Laravel group get the group's name:** a route without its own `->name()` inside a named
  group now gets the group's combined name, like Laravel does (`Route::prefix('x')->name('a')->group(fn () =>
  Route::name('.b')->group(fn () => Route::get('/', ...)))` gives `a.b`), so `route('a.b')` is no longer reported as
  unknown by `--format ziggy`. A route file that is `require`d but left out of the scan (typically a gitignored,
  local-only file, e.g. `if (app()->isLocal()) require __DIR__.'/local.php';`) is read from disk and its routes count.

## [1.0.0] — 2026-11-16

### Added

- **Package recognition (composer, npm, yarn, pnpm, bun):** callgraph reads the manifests and lock files (offline) and
  `callgraph.sh profile` shows a `stack:` line with the notable packages and their locked versions (the declared range
  without a lock, marked); the MCP startup profile gets a compact version of it. `profile --packages` lists every
  direct composer / npm dependency (installed and declared version, dev flag).
- **See which package a call goes to, and where a package is used:** in the outline, calls into a dependency are
  grouped under the package's short name (`guzzle: GuzzleHttp\Client::request`), the JSON output has a `package`
  field (versions once per file), and `locate` gives a "package" reason. `callgraph.sh uses <package> [--calls]` (MCP
  `package_usage`) lists per API symbol how often it is used and the first file; `(type)` marks an import without a
  call, `--calls` lists every call line.
- **`callgraph.sh deps`** (MCP `project_profile` with `deps: true`): probably unused, missing and dev-only composer /
  npm dependencies, one line each with a short reason. A false alarm can be silenced with `deps-ignore <name>` in
  `.callgraphignore`.
- **Inertia pages:** a PHP `Inertia::render('Board')` call links to its page file (`Pages/Board.vue`), the page's
  outline gets a `rendered by:` line, and `show . --format inertia` lists each page with its render sites and props,
  marking a missing page file or props the page does not declare in `defineProps`, plus the pages never rendered.
- **Laravel routes and Ziggy:** callgraph reads the route table from the loaded `routes/*.php` files (groups,
  resources, names, the `/api` prefix). A Vue / JS `route('events.show')` call in the outline shows the method, URI and
  controller action (or `⚠ no route`), and a controller method gets a `route:` line. `show . --format ziggy` lists
  unknown route names and route files that are never loaded; when some routes have dynamic names, unknown names are
  only marked `?`.
- **Laravel form check:** an Inertia `useForm({...})` form is compared with the validation of the controller action
  it posts to (FormRequest or inline `validate`): a `form:` line under the sending function and under the action
  (fields sent but not validated, required fields not sent, or `N fields ✓`); `show . --format forms` lists them all.
- **Laravel model summary:** under an Eloquent model the outline shows `table:` (the columns from the migrations) and
  `fillable:` lines, relation lines show their target, and `⚠` lines warn about a fillable or cast that is not a
  column. `show . --format models` lists the models, the tables without a model, and a table created twice without a
  drop (a fresh migrate would fail).
- **Fast single-file outlines in Laravel projects:** the outline of one file reads only the route, migration and form
  files it needs and keeps the results in `.callgraph/laravel.bin` (only if `.callgraph/` exists; checked against the
  files' modification time and size, rebuilt automatically, safe to delete). Route files are read in path order, so
  a controller's `route:` lines come in the same order in every view.
- **Code generator `callgraph.sh gen '<expr>'`:** writes PHP classes / members, Vue single-file components (script
  setup, template, style), composables, plain JS / TS modules and full HTML pages from a short Emmet-like expression,
  in the project's style (namespace from `composer.json`, imports from the project, TS or JS, semicolons,
  indentation). In a Vue script `use:{useX}@/composables/useX` is a named import (a bare path becomes one by itself
  when the file only has `export function useX`), `type:Flag{flag:str,on?:bool}` an interface (inline object types
  such as `list<{a:str}>` work too), and `raw{…}` / `const:{a,b}=useX()` write a statement as is.
  Targets: `--write <file>` (new file only), `--into <file>[::Class] [--after <method>]`, `--replace <file>:L<a>-L<b>`
  or `::<method>`, where `--expect <sha256-prefix>` refuses the change if those lines changed meanwhile; without a
  target, or with `--dry`, it only prints the code. It answers with one line per file. The new text is syntax-checked
  before writing (nothing is written on an error), every change to an existing file is backed up to
  `.callgraph/backups/` first (newest 10 per file), and `callgraph.sh undo [<file>]` restores the newest backup.
- **Generator: JS / TS modules and HTML pages:** `mod:util>fn:join(p:list<str>):str{…}+const:MAX:int=10` writes a
  module, `class:Svc<Base>[Iface]>…` a class (members as in PHP: `+` public, `-` private, `#` protected). Everything
  is exported by default, `-fn:` keeps a function private, `!fn:` / `!class:` makes it `export default`; a `.ts` file
  gets types, a `.js` file JSDoc. `--into x.ts[::Class]` adds members, `--replace x.ts::fn` or `x.ts::Class::method`
  replaces one. `html:Title[lang=hu]>(head>…)+(body>…)` writes a full document (doctype, charset, viewport, title)
  with the head shortcuts `meta:description=…`, `link:style.css`, `script:app.js` and `base:/app/`; any element and
  attribute works, and inline `style{…}`, `script{…}` and `comment{…}` are allowed (CSS and JS checked before
  writing). `--into x.html` adds at the end of `<body>` or into `::#id` / `::.class` / `::tag`,
  `--replace x.html::#id` replaces an element.
- **A Laravel feature in one step:**
  `gen 'laravel:Fee[auth,prefix=admin]>create(name:str,amount:int,paid?:bool)+edit+index'` writes the FormRequests,
  controller methods, routes and Inertia / Vue pages together, checks that the form fields match the rules, warns
  about NOT NULL columns without a default that the form does not send, and one `undo` reverts all of its files.
  Without Ziggy (no `tightenco/ziggy` / `ziggy-js`) the pages use plain URLs (`'/notes'`) instead of `route('…')`;
  a `str` field with a long-text name (body, content, description, …) gets a hint to use `:text`. `--expect` without
  `--replace` and `--after` without `--into` give a specific error (`--expect` is the sha256 prefix of the replaced
  lines, not of the whole file), and the help lists the field types.
- **`callgraph.sh rename route|class|method <old> <new>`:** renames across the whole project — route names
  (`->name()`, groups, `'profile.*'` patterns, resource routes via `->names()`, and every `route('x')` in PHP, Blade,
  JS and Vue), PHP classes (declaration, file move, namespace, `use`, type hints, docblocks; `--strings` also rewrites
  string references, `--keep-table` keeps the Eloquent table name) and PHP methods (overrides, resolved calls,
  callable arrays, route actions). Every changed file is syntax-checked first, `--dry` only reports, and one `undo`
  reverts the whole rename.
- **MCP `generate` and `rename` tools (opt-in):** listed only when the server runs with `--generate` or
  `CALLGRAPH_GENERATE=1`; without it the tool list stays exactly as before.

### Changed

- **MCP tool hints:** unlike every other tool, the opt-in `generate` and `rename` tools write files, so they declare
  `readOnlyHint: false` (and `idempotentHint: false`); all other tools still declare `readOnlyHint: true`.
- **Local stats and opt-in telemetry: new command kinds.** `uses` / `package_usage`, `deps`, `generate`, `undo` and
  `rename` are counted as their own commands in the local stats and event log (on this machine, `callgraph.sh
  stats`), and — only if you turned telemetry on — in the daily counts that are sent, still without names, paths or
  code. `callgraph.sh telemetry preview` shows what would be sent; off: `CALLGRAPH_STATS=0` (local),
  `callgraph.sh telemetry off` or `CALLGRAPH_TELEMETRY=0` (sending).

### Fixed

- PHP: a fully qualified name such as `\LogicException` or `\Helper` now resolves to the global namespace, not to the
  current namespace or an import.
- An SCSS file with accented characters before a function call (`// Fejléc háttér`) no longer crashes project-wide
  views such as `locate`, `profile` or the framework views (also in 0.9.5). If a language analyzer crashes on a file,
  only that file is skipped instead of the whole command failing.

## [0.9.5] — 2026-11-13

### Added

- **Local command log**: `callgraph.sh stats --events [--days N]` lists your recent calls (command, client, outcome,
  timing and size bands, how it relates to the previous call, and whether its file / symbol appeared in the previous
  call's output) and the most common command transitions. A "previous output reused" summary shows per transition how
  often the next call's file was the first file listed (`hit1`), the 2nd–5th (`hit5`), a later one (`hit`) or not
  there (`miss`), and whether its symbol was there (`hit` / `miss`). The log stays on this machine (14 days) and
  contains no names, paths or code; the reuse check keeps only the last call's output items, as keyed hashes in the
  local `session.json`, replaced at every call and never sent. Off: `CALLGRAPH_STATS=0`.

### Changed

- **Opt-in telemetry (schema 2)**: if you turned telemetry on, the daily counts now also include command-order
  counts (which command followed which, the gap between them, whether the target was the same file, folder or
  project), session-length bands, the target file's language and size band, and the previous-output bands per
  command transition (`hit1` / `hit5` / `hit` / `miss`, symbol `hit` / `miss`) — still no names, paths, code or
  hashes. You are told once; `callgraph.sh telemetry preview` shows exactly what would be sent.
- The session-start check may suggest sharing usage counts up to 10 times (every 2 days), never after you said no.
- **MCP tools say they only read, and when to use which:** every MCP tool now declares `readOnlyHint: true`,
  `destructiveHint: false`, `idempotentHint: true` and `openWorldHint: false`, since none of them changes anything or
  reaches outside the project, so directories that require these hints (e.g. OpenAI's) accept the server. The
  descriptions of `locate`, `outline`, `search` and `framework_view` now tell the agent which one fits: `outline` with
  `method` lists what that method calls (so there is no separate callees tool) and points to `find_callers` for who
  calls it.

### Fixed

- `callgraph.sh telemetry status` now lists `DO_NOT_TRACK` among the reasons sending is blocked.

## [0.9.4] — 2026-11-12

### Added

- **Session-start check** (optional `SessionStart` hook, `callgraph.sh hook install-session`; the installer
  asks): at the start of a new session it tells you when a recommended setting is off (refresh hook, MCP with
  `--auto-update`, opting in to shared usage counts) or a newer version is out, and your agent asks you before
  changing anything. At most every 2 days and 3 times per item; `hook never <item>`, `CALLGRAPH_RECOMMEND=0`.

- **MCP Registry**: each release now ships `callgraph.mcpb` (an MCPB bundle with the binaries for all
  five platforms) and publishes it to the official MCP Registry as `io.github.GeRiY/callgraph`, so MCPB-aware
  clients (e.g. Claude Desktop) can install the MCP server in one step, without a Rust toolchain.

### Changed

- Update checks and downloads go through `update.callgraph.eu` (falls back to GitHub); anonymous daily
  usage count — see "Anonymous daily count" in the README.
- `telemetry reset` (undecided again, still off) and `hook reset [item]` (undoes `hook never`); `DO_NOT_TRACK=1`
  now also stops telemetry.

### Fixed

- The release zips now keep the executable bit on the binary and scripts, so a manually unpacked
  package runs without `chmod +x` (the installer already set it).

## [0.9.3] — 2026-11-12

### Added
- **HTML files (`.html`, `.htm`):** handled as documents like Markdown — the outline lists the `<title>` and the
  `<h1>`–`<h6>` headings by level with line ranges, `--lines` works, and `--strings` searches the visible text and
  the `title` / `alt` / `placeholder` / `aria-label` / `value` attributes (`<script>`, `<style>` and comments are
  skipped). Broken or unclosed HTML does not stop the analysis.
- **`locate` finds properties and fields:** PHP properties (also `protected $fillable` and promoted constructor
  properties), TS / JS class fields and parameter properties, Rust struct fields, and Swift / Python / Go properties
  are definitions (`property $fillable  L12  ← "fillable" (exact property)`; `fillable`, `$fillable` and
  `Invoice::$fillable` all match); part of the name is a name-part match. A property defined in many files (like
  `$fillable` in every model) does not push the exact class or a rarer term's files down.

### Changed
- **`locate` ranks names that join several terms first:** a definition or file name containing two or more of the
  searched terms (`toggleFavorite` for `toggle favorite`, `PodcastEpisodeController` for `podcast episode`) gets a
  large boost that grows with the number of terms, even when each term alone is too general (the "too general" line
  stays). Definitions in type declaration files (`.d.ts`, `.pyi`) weigh less than real ones and no longer push these
  files to the top.

### Fixed
- **PHP: receivers from the Laravel container are resolved:** `$x = app(Foo::class)`, `resolve(Foo::class)`,
  `app()->make(Foo::class)`, `$this->app->make(Foo::class)` and `App::make(Foo::class)` give `$x` the type `Foo`
  (resolved through `use` and the namespace, like `new Foo()`), and `app(Foo::class)->bar()` /
  `resolve(Foo::class)->bar()` resolve to `Foo::bar`, so `--grep 'Foo::bar'` finds these callers without `--guess`.
  A string id (`app('foo')`) stays unresolved.

## [0.9.2] — 2026-11-12

### Fixed
- **JS / TS literal computed call target:** `o['x']()`, `o["x"]()` and ``o[`x`]()`` now get the same target as
  `o.x()` (the old `[]` target is gone); `o[k]()` stays a `dynamic_call`, a numeric key (`a[0]()`) no longer yields `[]`.
- **`--guess` on a single-level `--grep`:** `--grep 'Type::method' --guess` (and `--depth 1 --guess`) now also lists the
  unresolved, name-only callers, marked with `?` as in the `--depth 2+` view.
- **Single-level `--grep 'Type::method'` footers:** the `(+N unresolved, name-only matches skipped — --guess shows them)`
  and `(N of these callers contain dynamic calls — check them by hand)` lines of the `--depth 2+` view now also appear
  on the single-level view, when there is something to say.

## [0.9.1] — 2026-11-11

### Added
- **JSX component use is a call:** a capitalized JSX element (`<Button />`, `<Button>…</Button>`, `<Ui.Button />`) in
  `.tsx` / `.jsx` / `.js` is a `component_use` call (the same category as in Vue), resolved through the import like an
  ordinary function call, so `--grep 'Button$'` and `--depth` find the component's users; a component defined in the
  same file is an `internal_helper`. `<div>` and other lowercase elements, and fragments, are not calls.
- **Vue `--format api`** lists the component's `props:` (name, type if known, `[required]`) and `emits:`.

### Fixed
- **JSX in plain `.js` / `.mjs` / `.cjs` files** no longer gives `syntax error: Unexpected JSX expression` and an empty
  outline: these files are parsed as JSX-capable (as in CRA / babel / vite projects). `.ts` stays non-JSX (`<T>x` is a
  type assertion there).
- **Namespaced `--grep` pattern** (`App\Svc\Real::m`): a single `\` after an identifier character and before a capital
  letter is read as the namespace separator instead of a regex escape (`\S`, `\R`, `\B` used to match silently
  wrong or fail); `\.`, `\d`, `\w`, `\b` and a doubled `\\` are unchanged. A still-invalid pattern gets a 💡 line
  explaining the doubling. Same for `--depth` and `callers`.
- **`--layer` / `--layers` / `--by-layer` without a path** run on the project root (like `profile`) instead of
  printing the usage line.
- **The `protect` 🔒 mark** now also shows in `--format summary` (and the `stats` header); json, mermaid and dot are
  unchanged.

## [0.9.0] — 2026-11-11

### Added
- **`dynamic_call` marker:** calls whose target is decided at runtime used to be missing from the outline, so the graph
  looked complete where it was not. They are now listed (last, after the other categories) with the called expression
  as the target — PHP: `$obj->$m()`, `$obj->{$expr}()`, `$class::$m()`, `$fn()`, `new $class`, `call_user_func($x)`,
  `app($var)` / `resolve($var)` / `$this->app->make($var)`; JS / TS / Vue: `obj[expr]()`, `new (expr)()`, `import(expr)`,
  `require(expr)`; Python: `getattr(obj, name)()`, `globals()[name]()`, `importlib.import_module(name)`, `eval` / `exec`.
  Literal cases (`obj['x']()`, `app(Foo::class)`, `getattr(obj, 'x')()`) stay ordinary. It is a marker, not a caller
  edge: `--grep`, `--depth` and `locate` ignore it; `--depth` adds a line telling how many of the callers contain
  dynamic calls ("check them by hand"). In PHP, callback parameters typed `Closure` / `callable` and local closures (`$f = fn …`) do not count as dynamic calls, and `($this->invokable)()` on a class-typed property is a plain `Type::__invoke` call. Filter with `--category dynamic_call`. Rust, Swift and Go: not yet.
- **Recovery hints:** an empty or wrong result now tells the next step instead of staying silent. `--grep` with no match
  explains that the pattern is a regular expression on the call target, suggests the name-only pattern
  (`(^|::|\.|->)name$`, `--guess`) and `did you mean: User::find`; `Not found: <path>` and `No such method` add a
  second `Did you mean: …` line (up to 3 similar paths / method names); `--depth` says whether the target does not exist
  or exists but nothing calls it; `--strings` with no match explains the matching rule; `locate` with no match suggests
  name parts, English code names and `--strings`. An unknown `--category` is now an error listing the known categories
  (`Unknown category: … (known: …)`) instead of an empty result. The hints appear in the text output only; json,
  mermaid, dot, stats and the `next` suggestions are unchanged.
- **Recipes in the skill:** a short "Recipes — task → call sequence" section (bug fix, who calls this, unfamiliar
  project, where is this text) in the skill instructions, and the same in 3 lines in the MCP instructions.

## [0.8.0] — 2026-11-11

### Added
- **`--help` in the binary:** `callgraph --help`, `-h` and `help` print the full, current help (English or Hungarian)
  — the same text as `callgraph.sh --help`, now also mentioning Python, Go, `--signatures`, `protect` and the
  environment variables.
- **Python (phase 1):** `.py` / `.pyi` files in the map (tree-sitter): classes with bases and decorators, functions,
  methods (`async`, `@staticmethod`, `@classmethod`, `@property`), constants, `__all__`, docstrings, signatures
  (`--signatures`, `--format api`), imports, categorized calls (`native_python`, `internal_helper`, `static_call`,
  `external_call`), strings, syntax errors, Django / Flask / FastAPI detection. `venv`, `.venv`, `site-packages` and
  `__pycache__` are skipped by default.
- **Go (phase 1):** `.go` files in the map (tree-sitter): structs (fields as properties, embedded types), interfaces,
  type aliases and defined types, functions and methods with receivers (also when the type is declared in another
  file), generics, `const` / `var`, imports (alias, blank, dot), signatures with several return values
  (`--signatures`, `--format api`), categorized calls (`native_go`, `internal_helper`, `static_call`, `instantiation`,
  `external_call`; `go` / `defer` included), strings, doc comments, exported (capitalised) = public, syntax errors,
  Gin / Echo / Fiber / Chi / gRPC / GORM detection. `testdata` is skipped by default (`vendor` already was);
  `_test.go` files rank as tests in `locate`.

## [0.7.0] — 2026-11-11

### Added
- **`protect` lines in `.callgraphignore`:** `protect migrations/` keeps the matching files in the map (fully
  queryable) but marks them `🔒` in the outline, `--body`, `--lines`, `locate` and `--format json` (`"protected": true`):
  protected, read them, don't edit them. Same pattern rules as the exclusions; no `protect` line, no change.
- **Signatures in outlines:** `--signatures` (or `CALLGRAPH_SIGNATURES=1`) prints the compact signature on the method
  line of the outline and the `--grep` caller outlines: `updateOrders(array $ids, bool $notify = false): array  L4-9`
  (where the language provides params / return type). `--format api` now also shows TS / Vue return types, and for PHP
  a more specific DocBlock type (`@return array{updated:int}`, `list<int>`, `@param` shapes), at most 80 characters.

### Changed
- **Non-UTF-8 sources are decoded, not replaced with `�`:** ISO-8859-2, windows-1250 and ISO-8859-1/windows-1252
  files are detected and read properly (display, outline, `locate`, `--strings`); garbage still falls back to `�`.
- **Stronger change detection (Linux / macOS):** the freshness stamp also includes the inode change time, so a
  same-size edit with a restored mtime is noticed. The stored graph is rebuilt once, quietly.

## [0.6.1] — 2026-11-11

### Fixed
- **Files that are not UTF-8 (e.g. ISO-8859-2 / Latin-2) no longer give an empty result:** `--lines` and `--body` /
  the MCP `method_body` printed nothing at all for a file with accented text in another encoding (even in a single
  comment), with no message. The lines are now printed, with the bytes that are not valid UTF-8 shown as `�`; the
  line numbers stay correct.

## [0.6.0] — 2026-11-10

### Added
- **Suggested next step (opt-in):** with `CALLGRAPH_NEXT=1` or `--next` (MCP: `callgraph mcp --next`), a result can
  end with one line like `↪ r7 possible next: [1] OrderService.php::refund --body  [2] callers of refund  [3] outline
  OrderService.php`. The agent runs one by number (`callgraph.sh next r7 1`, several: `next r7 1,3`; MCP: the `next`
  tool) instead of composing the command. Only read-only navigation, only names that appear in that output, at most 3,
  and no line at all when nothing is clear (errors, empty results, json / mermaid / dot). Stored in the project's
  `.callgraph/results/` (last 50, 24 hours); `callgraph.sh stats` shows how often a suggestion was used.
- **Opt-in sending of the usage stats:** `callgraph.sh telemetry status|preview|on|off`. Nothing is sent unless the
  user turns it on in a terminal (`telemetry on` asks y/N; an agent cannot do it). Then the closed days' banded
  counts (the same as `stats`, no paths or names) go out once a day in the background; `CALLGRAPH_TELEMETRY=0` blocks it
  for good. The installer asks once (Yes / Not now / Never), only in an interactive terminal.

- **Note marker on hits:** a `📝 N` at the end of the line shows that a hit has notes — in `--grep` callers (outline,
  one-line-per-call view, every `--depth` level; MCP `find_callers`), in `locate` file lines and in `--format api`
  method lines. Method notes count on the method line; if the method has none, the file's notes. Orphaned notes do
  not count; without notes the output is unchanged.

### Changed
- **`stats` separates user errors from internal errors:** usage mistakes, missing files and unsupported files are
  counted as "user err" (not logged to errors.jsonl since 0.5.1); "internal" errors are also in errors.jsonl.
  An unsupported / generated / binary file in `callgraph.sh` is now a user error (no error-id).
- **One-time notice:** the first time local stats are recorded on a machine, one line on stderr says so (and how to
  turn them off). Never in MCP output.

## [0.5.3] — 2026-11-09

### Added
- **`--strict` for CI:** a file with a syntax error still gives exit code 0 by default (the partial map is useful).
  With `--strict` (or `CALLGRAPH_STRICT=1`), `show` (file, folder, `--grep`), `locate` and `profile` exit with code 3
  when a file they read has a syntax error; the output is the same.
- **`mail` and `notification` layers:** Laravel mail classes (`extends Mailable`, usually in `app/Mail`) and
  notifications (`extends Notification`) get their own layer instead of "unknown". Like `event`, they need this proof
  in the code: a folder that is just named `Mail` is not enough.

### Changed
- **The `flow:` line only shows real call pairs:** every `A → B` in the chain now means A really calls B. A layer
  that only calls into the middle of the chain is shown as a side branch, e.g.
  `flow: controller → service → model; command → service` (before: `controller → command → service → model`, although
  controllers never call commands).
- **Enums get no role from their name:** `app/Enums/PaymentProvider.php` was a "provider" just because of its name.
  An enum (PHP, TS, Rust, Swift) never gets a role from its name now; an `Enums` folder is shown as `enum`.
- **Copied framework cores are skipped like vendor code:** a framework copied into a legacy project (CodeIgniter
  `system/`, Yii `framework/`, Kohana `system/`) no longer adds `command` / `handler` / `request` layers to the
  profile. Only the framework core is skipped, the application code next to it still counts. A big subtree you want
  left out completely can go into `.callgraphignore` as a path (`sn_monitoring/`).

### Fixed
- **Syntax errors no longer hide the code after them (JS / TS / Vue, Rust):** before, only the functions before the
  error were mapped; in a Vue `<script setup>` an error in the first function could leave nothing. Now mapping goes
  on from the next top-level function, class or item, with the right line numbers; only the broken part is missing.
- **`--layer` / `--layers` / `--by-layer` now agree with the profile:** they classified files from folder names only,
  so `app/Actions/Event/*Action.php` showed up as `[event]` although the profile did not list an event layer. They now
  use the same rules and code proof as `profile` (from the project map; without it, a folder like `Event` gives the
  next role, e.g. `service`). Files skipped as vendor or build output get no layer here either.
- **`callgraph.sh --project <dir>`:** a folder path (absolute or relative) now works with every command, without
  registering it; before, file queries said "No projects.json — register a project first". An unknown alias still
  gives an error.

## [0.5.2] — 2026-11-09

### Added
- **`update --dev`:** installs the newest build of the `develop` branch (version `X.Y.Z-dev.<commit>`, checked
  against its `SHA256SUMS` like a release); `update --check --dev` only tells you whether there is a newer one. A plain
  `update` takes you back to the latest stable release. The daily background check and the MCP `--auto-update` stay
  on stable releases. On a dev build `callgraph --version` shows the dev version.

### Changed
- **The changelog in Hungarian too:** `CHANGELOG.hu.md` has every release in Hungarian, and the Hungarian website
  shows it.

### Fixed
- **`update --check` right after a release:** for a few minutes after a new release it could still say "up to date",
  because GitHub briefly served the old answer. It now asks without the cache and also looks at the list of recent
  releases, so a new version is found immediately.

## [0.5.1] — 2026-11-09

### Added
- **Local usage stats:** `callgraph.sh stats` shows how you (and your agents) used callgraph in the last 7 days
  (`--days N` for up to 90): how many calls per command and MCP tool, how many failed, how long they took and how big
  the output was, and for `locate` how often the first result was the file the agent opened next. Everything stays on
  your computer: nothing is sent anywhere, and no paths, names or search terms are recorded — only counts in ranges.
  The data is in the skill folder (`stats/`, about 90 days); turn it off with `CALLGRAPH_STATS=0`.
- **`--no-gitignore`:** add it to any command (show, folder views, `locate`, `profile`, `refresh`, `callers`) to also
  map the files your `.gitignore` excludes; for the MCP server and the hook set `CALLGRAPH_GITIGNORE=0`.
  `.callgraphignore` and the always-skipped folders (`vendor`, `node_modules`, …) still apply.
- **`locate route::events.show`:** finds a route by its name: the definition (`->name('events.show')` in
  `routes/*.php`) comes first, then every `route('events.show')`, `to_route(…)` and `redirect()->route(…)` in PHP and
  JavaScript / TypeScript / Vue. If nothing is found, it says what it looked for.
- **`callgraph.sh --version`** (also `-V` and `version`) prints the version, the platform and where the binary is.

### Changed
- **`.callgraphignore` accepts paths:** a line like `md`, `.md` or `*.md` still skips an extension, and every other
  line is now a path pattern written like in `.gitignore` (`public/build/`, `/storage/`, `!keep.js`), relative to the
  folder of the file. A bare word such as `build` is still an extension; write `build/` for a folder.
- **`locate` ranks exact matches first:** a file whose class, component or function name (or file name) is one of the
  terms now always comes before files that only contain the terms as text, such as translation files or build
  output.
- **A quoted phrase is one `locate` term:** `locate "Organizer's other events"` searches for the whole text in the
  strings of the code (labels, translations) instead of splitting it into words that matched unrelated names.
- **`locate` always shows a `root:` line** and paths relative to it, also when only one file matches.
- **The profile lists the main layers first:** the architecture layers (controller, service, model …) come first, by
  file count, and tests, migrations and configuration come last, so a main layer such as `model` no longer hides
  behind `… +6 more layers` while layers with one or two files are shown.
- **`--by-layer` shows a header for each layer:** every group of callers starts with a line such as
  `[service]  3 calls`, in the normal and in the short ("one line per call") `--grep` view, and in every level of
  `--depth`. The group closest to the called method comes first (model → repository → service → controller), as
  the help says; before, only a `[service]` tag at the end of some lines showed the layer.
- **User mistakes are no longer logged as tool errors:** a mistyped layer name, an unknown option or a file that does
  not exist now prints just the message — no `[error-id: …]` line, no bug-report link, and nothing is written to
  `errors.jsonl`. Real internal errors are still logged and get an error-id.
- **JSON outlines show the values:** `scripts.build = "vite build"` and `devDependencies.vite = "^6.0"` instead of
  just `string`; numbers, `true` / `false` and `null` as they are, strings over 60 characters cut with `…`, and
  objects and lists with their size (`object (3)`, `array (2)`).
- **Read one JSON key:** `callgraph.sh package.json::scripts --body` prints that key with its exact lines; use dots
  for deeper keys (`scripts.build`) and numbers for list items (`files.0`). Before, it said "No such method".
- **`locate` finds JSON keys:** `locate laravel-vite-plugin` now lists `package.json` with the line of the key. Only
  the exact key name (or the whole dotted path) counts, so searches for other words do not fill up with JSON files.
- **Markdown headings show their level:** the outline indents sub-headings and marks them with `#`, `##`, `###`, and
  no longer writes `()` after a heading (`## Docker / build  L51-61`).
- **CSS and SCSS rules are no longer written like functions:** `.p-toast  L29-31` instead of `.p-toast()`; mixins and
  functions keep their `()`.

### Fixed
- **CSS outlines list every rule:** some rules were missing (a rule with the selector `.p-toast` was always left out,
  and so was the first rule after `@charset`), and at-rules were not shown at all. Now every rule and at-rule is listed with its
  lines: `@import`, `@tailwind`, `@media`, `@supports`, `@layer`, `@keyframes`, `@font-face`, and the Tailwind v4
  `@theme`, `@utility`, `@custom-variant`, `@variant`, `@plugin`, `@source` and `@config`.
- **CSS rules start on the right line:** a rule after a comment or another rule showed the line where the previous
  rule ended (`.p-datepicker-panel  L31-41` instead of `L39-41`).
- **CSS paths with `/*` inside quotes stay whole:** `@source '../**/*.blade.php'` was cut, because the `/*` in the
  path was taken for the start of a comment.
- **PHP and JavaScript layers are no longer mixed up in the profile:** in a project with PHP controllers and
  AngularJS (or other JavaScript) controllers, the profile showed one `controller` layer with all of them. A role
  that exists in more than one language is now shown apart, e.g. `controller (php)` and `controller (js)`, and the
  call direction (`flow:`) no longer joins the backend and the frontend through a shared name such as `service`.
  Projects with one language look exactly as before. `--layer controller` still finds both; `--layer controller:js`
  (or `--layer "controller (js)"`) only the JavaScript ones.
- **Third-party code no longer makes up the project's architecture:** an SDK or library copied into the project (a
  subfolder with its own `composer.json`, `package.json` or `LICENSE`, or folders such as `vendors/`, `third_party/`,
  `extensions/`, `external/`, `libs/`) gave the profile layers and patterns (Builder, Factory, Handler…) that the
  project itself does not have. These files are now left out of the profile, and one line says so, e.g.
  `vendor: 312 files skipped (extensions/Facebook, vendors/phpoffice, …)`. Searches and folder views still see them.
  `profile --with-vendor` counts them again.
- **Copies of build output are left out of the profile:** folders named `dist`, `build` or `*.min` (such as
  `web/assets.min`) and minified files no longer count twice as layers; a `generated: N files skipped (…)` line shows
  them.
- **A folder named after a business topic is not an event layer:** `app/Services/Event` or a JavaScript
  `controllers/event` folder became an `event` layer. `event`, `listener` and `job` now need proof in the code — an
  Event / Listener / Subscriber base class or interface, Laravel's `Dispatchable` or `ShouldQueue`, or a `handle(…)`
  method in a `Listeners` namespace; otherwise the file keeps its other role (`service`, `controller`, `model` for
  `app/Models/Job.php`).
- **Files your `.gitignore` excludes no longer show up:** the build output of a Laravel + Vite project
  (`public/build/assets/app-CBW7lKWe.js`, listed in `.gitignore` as `/public/build`) appeared in `locate` and folder
  views. `.gitignore` now works the way git reads it: patterns with a leading `/`, nested paths, `**`, `!` exceptions
  and a `.gitignore` in any subfolder. The same rules apply everywhere callgraph looks at files, including the MCP
  server's file watcher and the editor hook.
- **Minified and bundled JavaScript / CSS is skipped even without a `.min.` name or a `.gitignore`:** a file whose
  first lines are extremely long is recognised as generated and left out.
- **`locate --orange-500`** (a CSS variable) was rejected as an unknown option; it is now searched for. The known
  options (`--all`, `--project`) still work as before.
- **`callgraph.sh --version`** printed the usage of `show` and logged an error.
- **Files with a syntax error are no longer shown as if they were complete:** a file you are in the middle of editing,
  or one with a merge conflict, could come back with no functions at all (JavaScript, TypeScript, Vue, Rust) or
  without any notice (PHP, Swift). A search then said "no such method" or found no callers, which could make an agent
  think the code was unused. Now the parts before the error are kept (functions `a` and `b` stay visible when `c` is
  broken), and every result says so: `⚠ syntax error at L3: … — this file's outline may be incomplete` under the file,
  a closing line such as `⚠ 2 files had syntax errors (results may be incomplete): src/tail.js L3, src/v.vue L4` when
  a folder, a search (`--grep`, `--depth`), `locate` or `profile` read a broken file, `parseErrors` in `--format
  json`, and a warning on the error output of `callers`. The exit code stays 0, because the rest of the result is
  still correct and useful. Covers JavaScript, TypeScript, Vue, PHP, Rust, Swift and JSON (CSS, SCSS and Markdown
  are read leniently and are not flagged).

## [0.5.0] — 2026-11-06

### Added
- **Where to start with a task — `locate`:** give it the names and terms from a task in one go (`callgraph.sh locate
  RefundController processRefund refund payment_status checkout.vue`, or the new `locate` tool over MCP) and it lists
  the few files worth opening first, best first, each with the reason: the class or method of that name, the file of
  that name, a name that contains the word (`refund` → `RefundJob`, `refunds_table`), a text such as a column name or
  route, or a call to it. It answers from the project map without reading the code again, so it is quick. A file that
  matches several of your terms comes before one that matches a single term many times. It shows 5 files (`--all`
  for every match) and says which terms found nothing and which are too general. The skill and MCP instructions now
  also tell agents when to use it, and when plain text search (grep) is the better tool.
- **Project profile:** `callgraph.sh profile` tells you in about 15 lines how an unfamiliar project is built: the
  architecture (Laravel / Yii layered MVC, DDD / hexagonal, Vue / React frontend, SwiftUI MVVM, Rust web), which folder
  holds which layer (controllers, services, repositories, models, components, stores…) with file counts, the design
  patterns it uses (Repository, Factory, Strategy, Observer…), the folders with no clear role, and whether the layers
  call each other in the expected direction. It reads folder and file names and checks them against the code graph
  (`extends Controller`, `implements *RepositoryInterface`, `defineStore`); what it cannot tell stays "unknown".
  `--full` shows every folder. For AI clients: the `project_profile` MCP tool, and a short profile is sent when a
  client connects.
- **Layers in queries (opt-in, nothing changes by default):** `--layer service` searches only the files of that layer
  (no need to know the folder names), `--layers` groups the summary by layer and marks each file with its role,
  `--by-layer` orders "who calls it?" results by layer, closest to the called code first (also for `callers`, and as
  `layer` / `by_layer` on the MCP tools). Folder names that say nothing, or are not English, can be mapped in a
  commit-able `.callgraph-layers` file (`service = app/Logic`, `ignore-role = app/Legacy`).
- **Exclude extensions per project:** a `.callgraphignore` file in the project root (or in any subfolder; the nearest
  one applies) lists extensions callgraph should skip there, one per line (`md`, `.json` or `*.vue`; `#` starts a
  comment). Commit it, and everyone on the team gets the same results. A file you name directly with an excluded
  extension is refused with a clear message.

### Changed
- **Only supported files are processed, more strictly:** callgraph already ignored files with other extensions; now
  it also skips generated files (`package-lock.json`, `*.min.js`, `*.min.css`) and files with binary content, even
  when their extension is supported. Naming one of them directly gives a clear reason instead of a result.
- **Upper-case extensions count:** `B.PHP`, `App.TS` or `Main.Vue` are now processed like their lower-case versions
  (before, they were silently skipped).

### Fixed
- **A file reached through a symlink is no longer listed twice:** if a symlink pointed to a file inside the project,
  that file appeared (and was analysed) twice in folder results.
- **Windows: the short summary shows file names again:** when a folder's output was too long and callgraph showed the
  one-line-per-file summary instead, Windows listed each file with its full path instead of just its name.

## [0.4.0] — 2026-11-06

### Added
- **Refresh the project map ahead of time:** `callgraph.sh refresh` brings the project map in `.callgraph/` up to
  date right away — every changed, new or deleted file, or only the files / folders you name — and prints a one-line
  summary. Handy after a `git pull` or after an agent wrote many files: the next query no longer has to re-analyse
  them. `--background` returns at once and does the work in a separate process. Two refreshes never work on the same
  project at the same time; if one is already running, the other simply skips.
- **Optional Claude Code hook:** after Claude edits or writes a file, callgraph refreshes that file in the background,
  so the next question about it is answered instantly. The installer asks whether to add it (`install.sh --hooks`
  adds it without asking); it only adds one entry to `~/.claude/settings.json`, keeps all other settings and hooks,
  and saves a `settings.json.bak` backup first. Check or remove it any time: `callgraph.sh hook status` /
  `callgraph.sh hook uninstall`. It does nothing outside registered projects (or ones that already have a
  `.callgraph/` folder).
- **The MCP server keeps itself up to date:** while `callgraph mcp` runs, it watches the projects it has opened and
  refreshes changed files a moment after they are saved, so queries stay fast. The answers are exactly the same as
  before. If watching is not possible on a system, it says so once and works as before. To switch it off, set
  `CALLGRAPH_WATCH=0`.
- **The MCP server explains itself:** AI tools that use callgraph through MCP (Claude Code, Claude Desktop, Cursor,
  VS Code) now get a short usage guide when they connect, in your language: which tool to use when, how to find every
  caller of a function, and how to read the answers. A new `guide` tool gives them the full manual on request. The
  tool descriptions now each carry an example and the most common mistake to avoid. The guide is made from the same
  text as the skill's `SKILL.md`, so the two always say the same thing.
- **The installer can set up the MCP server for Claude Code:** if the `claude` command is available, `install.sh`
  asks whether to register callgraph as an MCP server (`install.sh --mcp` does it without asking, `--no-mcp` skips
  it). Running it again does not create duplicates. It prints how to remove it (`claude mcp remove -s user callgraph`)
  and where to find the setup for Claude Desktop, Cursor and VS Code. If this step fails, the rest of the install
  still completes. The installer now ends with a reminder to restart Claude Code.
- **The MCP server can update itself:** add `--auto-update` to the server's arguments in your AI tool's MCP settings
  (`"args": ["mcp", "--auto-update"]`, recommended). When the tool starts the server — at most once a day — it checks
  for a newer callgraph release, downloads it, checks that the file is exactly the published one (it never installs
  a file whose checksum does not match), installs it just like `callgraph.sh update` (your projects and settings are
  kept) and restarts itself, so the tool works with the new version right away. If anything goes wrong (no
  internet, GitHub limit, a bad download, no write permission) it simply starts the version you have and writes one
  line to the tool's log. It only writes into callgraph's own folder, never into your projects.
- **The MCP server tells you about updates:** without `--auto-update`, the server checks for a newer release at most
  once a day in the background (it never makes a request wait) and adds one line to the first answer of the session:
  `ℹ callgraph X is available (installed: Y) — update: …/callgraph.sh update, or add --auto-update to the MCP
  config`. `CALLGRAPH_UPDATE_CHECK=0` turns off all update checks (no network at all); they are also off in CI.

### Changed
- **Comments inside a method stay in `--body`:** the short method view used to hide comment lines inside the method
  too, so an agent could think a line was missing and read the file again. Now only the docblock above the method
  and blank lines are left out; comments inside the method (they often explain the intent) are shown. When a
  docblock was left out, the header line says so, e.g. `(docblock L112-119 omitted: --doc)`. `--doc` still adds the
  docblock, `--raw` still prints the exact source. The MCP `method_body` tool works the same way.
- The project website has its own domain: https://callgraph.eu (the old `geriy.github.io/callgraph` address redirects).

### Fixed
- **"Who calls it" now finds calls from child classes:** searching for a method with its class name
  (`--grep FrontendController::redirectToNewDashboard`, or the MCP `find_callers` tool) missed the places where a
  child class calls the inherited method as `$this->redirectToNewDashboard()` — only the bare method name found them.
  Now these calls are found too, marked with the child class (`L139 via EventeditController │ …`). A child class that
  has its own version of the method is not counted, because its call runs its own version.

## [0.3.0] — 2026-11-06

### Changed
- **Faster queries on big projects:** callgraph now keeps a compact project map in `.callgraph/` in just three
  files, however big the project is: a short list of all files, one file with every file's full analysis, and one
  with the summary that searches need. It is updated file by file, only for files that changed since the last query
  (deleted or renamed files drop out). "Where is it called" (`--grep`, every `--depth` level), `callers`,
  `--strings` and `--const` are answered from the summary file in one read; a single-file view (outline, `--body` …)
  reads just that file's part of the full-analysis file. The output is exactly the same as before.
  An edited file shows up in the very next query; anything damaged or written by another callgraph version is
  rebuilt quietly. The map replaces the old analysis cache (`cache.jsonl`), which is removed automatically, so the
  folder takes less space than before. The MCP server (`callgraph mcp`) keeps the map in memory between requests.
  To switch it off (e.g. to compare), set `CALLGRAPH_NO_GRAPH=1`. Details: `docs/PROJECT-GRAPH.md`.
- **Shorter call lists in "where is it called" results:** when one caller method makes the same call on several lines
  with the very same source text, it is now shown once with all its line numbers, e.g.
  `L12, L40 │ $this->repo->find($id)`. Each caller shows at most 3 such rows; the rest are listed by line number only:
  `… +2 more: L55, L61`. This applies to `--grep`, the first `--depth` level and the one-line-per-call view used when
  the result would be too long. Calls through an interface stay on their own row (`via …`).
- **Shorter method bodies:** `--body` now leaves out what an agent rarely needs: the docblock / doc comments above the
  method, blank lines and lines that are only a comment. Code lines are printed exactly as written (a comment at the end
  of a code line stays), and nothing inside a string, heredoc or template literal is ever removed. The line numbers are
  still the real ones, so a gap shows where lines were skipped and you can still read an exact range. Attributes such as
  `#[test]` / `#[Route(...)]` are kept. Works for PHP, JS / TS / Vue, Rust and Swift, for several methods at once and
  for `--grep … --body`.
- New `--doc` adds the docblock back; new `--raw` prints the exact, unfiltered source (as before, plus Rust / Swift
  doc comments and attributes). The MCP `method_body` tool follows the same default and has `doc` / `raw` options.

### Added
- **Notes on files and methods (pilot):** `callgraph note add <file>[::<method>] "<text>"` (also
  `callgraph.sh note …`) attaches a short note, stored outside the source in `<project>/.callgraph/notes.jsonl`
  (git-ignored) with the date and time (UTC). The outline shows it under the file header or the method
  (`📝 2026-11-05 20:27 · text`, newest first, at most 3), `--body` above the method source. `note list [<path>]`
  lists them (with `[orphaned]` when the method is gone), `note rm <id>` deletes one, and
  `show <dir> --notes <pattern>` searches them. The anchor is the method name, not a line number. MCP: a new
  read-only `notes` tool (there is no write tool). Without notes the output is unchanged. Details: `docs/NOTES.md`.
- **TypeScript: calls through typed objects and interfaces.** callgraph now works out which class a TS method call
  goes to when the type is written down: a typed parameter (`repo: Repo`), a typed class property
  (`private repo: Repo`, also `constructor(private repo: Repo)`), `this.repo = new Repo()` or `const x = new Repo()`.
  So `this.repo.find()` shows up as `Repo::find`, and `--grep 'Repo::find'` finds it. Classes record their
  `implements`, and TS interfaces (`interface X extends Y`) appear in the map, so searching
  `PlaylistResolver::getPlayables` also finds calls made through its `Resolver` interface, marked `via Resolver`
  (in `--depth` and `callers` too). When the type is unclear (no type, `any`, a union like `Repo | null`, a generic
  `T`, or different values in the same function) the call stays as before.
- **`--const` lists re-exports:** besides references and definitions, `--const NAME` now shows where a name is passed
  on, marked `[re-export]`: Rust `pub use state::{CheckState, STATE_FILE};` / `pub(crate) use …`, JS / TS
  `export { NAME } from './x'` / `export { NAME }`, PHP `use const App\NAME;`.

### Fixed
- `--grep <pattern> --body` prints the source of the caller methods again (the option was ignored).
- MCP `method_body`: `method` may be a single name as a plain string as well as a list (some clients send
  `"method": "parse"`, which used to fail with an "expected a sequence" error).

### Fixed
- **JS/TS: no more doubled methods when a class is named like its file** (`PlaylistResolver.ts` with
  `class PlaylistResolver`): the class's methods were listed twice, once under the file's `module` entry and once
  under the class. Now class members appear only under the class and top-level functions only under the module.

## [0.2.0] — 2026-11-05

### Added
- **Background update check:** at most once a day `callgraph.sh` checks GitHub in a detached background process
  (never slows a query down). If a newer release exists, the output ends with one line, once a day:
  `ℹ callgraph X is available (installed: Y) — update: bash …/callgraph.sh update`. Claude Code tells the user and
  offers the update; it never updates on its own. Opt out with `CALLGRAPH_UPDATE_CHECK=0`; off in CI.
- **Bug reporting via GitHub:** bilingual issue forms (bug report, idea / feedback), a private security report link and
  `SECURITY.md`. Error messages now point to the bug form; `feedback.sh show <error-id>` prints the details to paste.
- **English skill instructions:** `SKILL.md` now exists in English and Hungarian (`skill/skill-i18n/SKILL.<en|hu>.md`).
  The skill follows the language: `callgraph.sh set-lang <en|hu>` rewrites `SKILL.md` in that language (the other one
  disappears); the first install (`install.sh` → new `callgraph skill-sync`) and `update` keep the user's language.
  Restart Claude Code to reload the skill.
- **Release checksums:** every release has a `SHA256SUMS` asset (`sha256sum` format, all assets). `update` verifies the
  downloaded zip's SHA-256 before installing and installs nothing on a mismatch; a release without `SHA256SUMS` (v0.1.0)
  is refused unless `--insecure-skip-checksum` is given. `update --from <zip>` checks `--sha256 <hex>` or a
  `SHA256SUMS` next to the zip, otherwise it warns in one line and installs.
- **MCP server mode:** `callgraph mcp` runs a Model Context Protocol server on stdio (official Rust SDK `rmcp`), so
  Claude Desktop, Cursor, VS Code or any MCP client can use callgraph. Tools: `outline`, `method_body`, `find_callers`,
  `search`, `framework_view`, `projects` — the same compact text and output cap as the CLI. Read-only, no network,
  paths confined to the project root. The skill / CLI usage is unchanged. Setup recipes: `docs/MCP.md`.
- **See how something is called:** "where is it called" results (`--grep`, and every level of `--depth`) now show the
  call's own source line next to its line number, e.g. `L25 │ $this->repo->find($id)` — no extra `--body` call needed
  just to see the arguments.
- **Calls through interfaces and traits:** searching a class method (`--grep 'PlaylistResolver::getPlayables'`) also
  finds the calls made through its interface, abstract parent or trait (and, the other way round, through subclasses
  that don't override it). These are marked, e.g. `L14 via Resolver │ …`; `--depth` follows them to the next level, and
  `callers` adds a `"via"` field. Works where the call's receiver type is known (PHP: typed properties and other
  resolved receivers; Rust: `Type::method` calls); JS/TS interfaces and Swift protocols are not tracked yet.
- **Rust: smarter method calls.** For `x.load()` callgraph now works out the type of `x` in the simple cases
  (parameter type, `let x = Repo::new()`, `Repo { … }`, `self`, a field of your own struct like `self.repo`), so the
  call shows up as `Repo::load` and "who calls `Repo::load`?" finds it. When the type can't be worked out, the call is
  kept as `?::load` and a `Repo::load` search still lists it, marked as uncertain — instead of silently missing it.
- **Rust constants:** `--const` now finds Rust `const` / `static` items — where they are defined and where they are
  used (also `Type::CONST`).
- **JS / TS object methods:** functions inside an object literal (`const api = { fetch() {}, save: () => {} }`, or a
  plain `export default { … }`) are listed as separate methods (`api.fetch`) with their line ranges, and calls to them
  are found.

### Changed
- **Faster builds:** `tools/dev.sh check` / `test` / `clippy` use the debug profile (like CI now; `check-release` keeps
  the LTO variant), the dev container shares one build cache across all checkouts (`CALLGRAPH_DEV_TARGET`), and CI
  caches the compiled dependencies (`Swatinem/rust-cache`) and runs clippy + tests without LTO. The released binaries
  are built exactly as before (release profile, LTO).
- **Shorter result lists:** when the project root would repeat on many lines (`--grep`, `--depth`, `--strings`,
  `--const`, folder outlines, framework views…), the first line is now `root: /abs/path/to/project` and every path after
  it is relative to that root. Outputs with a single path, and the machine formats (`json`, `mermaid`, `dot`, `callers`),
  are unchanged.
- The committed / packaged default `SKILL.md` is now English (generated from `skill-i18n/SKILL.en.md`).
- `SKILL.md` now recommends finding callers by name (`--grep '(^|::|\.|->)name$'`), which also catches inherited,
  trait and unknown-receiver calls; `Type::method` is for narrowing a common name. It also explains the new search
  output (`root:` line, `L<n> │ code`, `via`, `?::`) and the new Rust / JS options.
- `--const` also lists the matching constant definitions (marked `[definition]`), not only the references.
- Version lookup for `update` / `update --check` uses the `releases/latest` redirect instead of the GitHub API, so the
  API rate limit (HTTP 403) no longer applies.
- The project page moved to the `gh-pages` branch (https://geriy.github.io/callgraph/); page edits no longer touch `main`.

### Fixed
- Windows: the project root is printed as `C:\…` instead of `\\?\C:\…` in search output.
- A bare method selector also finds an object-literal method: `store.ts::updateSongs` selects `playableStore.updateSongs`.
- `--grep … --depth`: from level 2 on, callers are searched in the whole project (from the project root), not only in
  the folders of the level-1 matches — callers in other folders were missed.
- Rust `--format api` now lists the `pub fn` methods of `impl` blocks (and trait implementations), the `pub` constants
  and fields — before, it showed no public methods for Rust types.
- `--strings` now finds texts inside Vue template expressions (`{{ … }}`, `:title="…"`, `@click="…"`) and in Rust
  `const` / `static` values, including byte strings (`b"…"`).
- SCSS: `@mixin` / `@function` parameters (`$color: t.$brand, …`) and `@include` keyword arguments were listed as
  constants; now only `$var: value;` declarations are (the parameters stay the mixin's params).
- `update --check` now also records its result in `update-check.json`, so the daily background check does not repeat
  it and a newer version found manually shows up in the notice.

## [0.1.0] — 2026-11-05

First release.

- Languages: PHP, JavaScript / TypeScript, Vue, **Rust**, **Swift / SwiftUI**, CSS / SCSS, JSON, Markdown.
- Structure, categorized call graph, method bodies, "who calls this?" and impact analysis (`--grep --depth`),
  framework views (Laravel / Yii), many output formats.
- English or Hungarian output (`callgraph.sh set-lang en|hu`).
- Built-in update: `callgraph --version`, `callgraph.sh update [--check|--force]`.
- Prebuilt packages for macOS (arm64, x86_64), Linux (x86_64, ARM64, static musl) and Windows.

[Unreleased]: https://github.com/GeRiY/callgraph/compare/v1.0.2...develop
[1.0.2]: https://github.com/GeRiY/callgraph/releases/tag/v1.0.2
[1.0.1]: https://github.com/GeRiY/callgraph/releases/tag/v1.0.1
[1.0.0]: https://github.com/GeRiY/callgraph/releases/tag/v1.0.0
[0.9.5]: https://github.com/GeRiY/callgraph/releases/tag/v0.9.5
[0.9.4]: https://github.com/GeRiY/callgraph/releases/tag/v0.9.4
[0.9.3]: https://github.com/GeRiY/callgraph/releases/tag/v0.9.3
[0.9.2]: https://github.com/GeRiY/callgraph/releases/tag/v0.9.2
[0.9.1]: https://github.com/GeRiY/callgraph/releases/tag/v0.9.1
[0.9.0]: https://github.com/GeRiY/callgraph/releases/tag/v0.9.0
[0.8.0]: https://github.com/GeRiY/callgraph/releases/tag/v0.8.0
[0.7.0]: https://github.com/GeRiY/callgraph/releases/tag/v0.7.0
[0.6.1]: https://github.com/GeRiY/callgraph/releases/tag/v0.6.1
[0.6.0]: https://github.com/GeRiY/callgraph/releases/tag/v0.6.0
[0.5.3]: https://github.com/GeRiY/callgraph/releases/tag/v0.5.3
[0.5.2]: https://github.com/GeRiY/callgraph/releases/tag/v0.5.2
[0.5.1]: https://github.com/GeRiY/callgraph/releases/tag/v0.5.1
[0.5.0]: https://github.com/GeRiY/callgraph/releases/tag/v0.5.0
[0.4.0]: https://github.com/GeRiY/callgraph/releases/tag/v0.4.0
[0.3.0]: https://github.com/GeRiY/callgraph/releases/tag/v0.3.0
[0.2.0]: https://github.com/GeRiY/callgraph/releases/tag/v0.2.0
[0.1.0]: https://github.com/GeRiY/callgraph/releases/tag/v0.1.0
