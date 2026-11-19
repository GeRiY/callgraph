# callgraph — Filament

`CG=<skill>/callgraph.sh`, as in [SKILL.md](../SKILL.md) (the core: start there). Filament 3 admin panels (`app/Filament/Resources/XResource.php`,
`app/Filament/Pages`, `app/Filament/Widgets`, `app/Providers/Filament/*PanelProvider.php`). Filament is Livewire-based, see also [livewire.md](livewire.md).

- **Overview**: `$CG app --format filament` (MCP `framework_view`, `view: "filament"`): a header with the counts, then per resource at most five lines:
  `PostResource  app/Filament/Resources/PostResource.php → Post(posts)`, `pages: index /admin/posts · create · edit /{record}/edit` (URIs under the
  panel path; the slug is `$slug` or the plural of the class name), `form: title*, slug*, author_id* (author)` (`*` = `->required()`, `(rel)` = `->relationship('rel')`),
  `table: title, author.name · filters: status · actions: edit, delete, publish · bulk: delete`, `relations: CommentsRelationManager`; then standalone
  pages, widgets (with the models they query) and panels, `⚠` lines and a `? N skipped: …` footer for names built at run time (`TextInput::make($name)`, `->path($x)`).
- **Checks**: a form field or table column must be a model column (migration), fillable / cast, accessor or relation; `author.name` needs the relation
  (and the attribute on its target model); `->relationship('x')` and `RelationManager::$relationship` must be relation methods of the model:
  `⚠ form field 'tilte' is not a column of Post(posts)  <file> L41`. Never reported: no migration or model found, `->statePath()`, `->dehydrated(false)`,
  placeholders / view fields, `getStateUsing` / `state()` columns, `*_count` / `*_sum_x` columns, repeater / builder items, a table with its own `->query()`,
  models with an unknown trait or `__call`.
- **Trace**: `$CG trace filament:posts` (also `PostResource`, `filament:Post`): the class, model + table + file, panel, pages with full URIs, form, table,
  filters, actions, relation managers (`→ comments`), findings.
- **From the model side**: every resource carries the key `filament::resource:Post`, so `$CG app --grep filament::resource:Post` finds the resource.
- Not covered: Filament 4 `Schema` classes, form / table schemas moved to other classes or `static function formSchema()`, `discoverPages` / `discoverWidgets`
  (all found classes are listed), Livewire components inside Filament pages, custom panel paths from config, multi-tenancy.
