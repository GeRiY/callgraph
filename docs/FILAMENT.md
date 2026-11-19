# FILAMENT — resource ↔ modell ↔ oldalak ↔ űrlap / tábla

Modul: `src/packages/filament/` (`scan.rs`: szövegszkennerek, `render.rs`: a kimenet szövegei és az ellenőrzések, `keys.rs`: a resource-kulcs a gráfban).
Nézet: `--format filament` (MCP: `framework_view`, `view: "filament"`) és a `trace filament:posts`. A projekt-szintű olvasás egyszer fut
lekérdezésenként, és csak akkor, ha a kimenet kéri; `app/Filament` / `app/Providers/Filament` nélküli projektben nem fut. A modell-tényeket (tábla, oszlopok a
migrációkból, fillable, cast, relációk) a Laravel `models` nézetének katalógusa adja (`Laravel::model_info`), ezért a nézet Laravel-projektet igényel.
Hibatűrő és konzervatív: találathoz statikus név, ismert modell + migráció és teljesen látható modellosztály kell; ami futásidőben épül, a lábléc
számlálójába kerül, soha nem figyelmeztetés.

## 1. Mit olvas
- **Resource**: `class XResource extends Resource` (`Filament\Resources\Resource`): `$model = Post::class`, `$slug`, `getPages()`
  (`'edit' => EditPost::route('/{record}/edit')`), `form()` (v3 `->schema([..])`), `table()` (`->columns`, `->filters`, `->actions` / `->recordActions` /
  `->headerActions`, `->bulkActions` / `->toolbarActions`), `getRelations()` (a `::class` hivatkozások).
- **Komponens**: minden `X::make('név')` hívás, ahol az `X` a `Filament\…` névtérből van importálva (`use Filament\Forms;` után a `Forms\Components\TextInput::make` is).
  A fluens lánc (`->required()->relationship('author', 'name')`) hívásonként olvasott; a beágyazott sémák (`Section` / `Grid` / `Fieldset` / `Tabs` / `Tab` / `Wizard` /
  `Step` / `Group` / `Split`) külön rekurzió nélkül, a szülő-argumentum tartalmazás alapján kapcsolódnak. A `Repeater` / `Builder` / `KeyValue` elemei és a modális
  űrlapok (`->form([..])`, `->createOptionForm([..])`) nem a resource mezői: nem listázottak.
- **Akció neve**: `Action::make('publish')` → `publish`; `EditAction::make()` → `edit`, `DeleteBulkAction` → `delete`, `ForceDeleteAction` → `force-delete`.
- **Oldal-URI**: `/{panel útvonal}/{slug}{oldal útvonala}`; a slug a `$slug` vagy az osztálynév (`Resource` nélkül) többes száma kebab-case-ben
  (`Blog/PostResource` → `blog/posts`). A panel: az, amelyik a `->resources([..])`-ben felsorolja, amelyik `->discoverResources(in: app_path('..'))` könyvtára alatt a fájl van,
  vagy az egyetlen panel. A panel-provider: `->id('admin')`, `->path('admin')`.
- **Önálló oldal** (`app/Filament/**/Pages`, `extends Page`, `Filament\Pages`), **widget** (`StatsOverviewWidget`, `ChartWidget` és társai, `TableWidget`, `Widget`; az `as BaseWidget`
  alias feloldva; a lekérdezett modellek a `Post::count()` típusú statikus hívásokból), **relation manager** (`$relationship = 'comments'`).

## 2. Ellenőrzések (⚠)
- Űrlapmező / táblaoszlop neve: oszlop (migráció), fillable / cast, accessor (`getFooBarAttribute`, `fooBar()` kis-nagybetű- és `_`-érzéketlenül) vagy reláció. `author.name`:
  a `author` reláció és — ha a cél-modell és a migrációja ismert — a `name` attribútum a cél modellen.
- `->relationship('author', ..)` és `RelationManager::$relationship`: a modell reláció-metódusa legyen.
- Üzenetek: `form field 'tilte' is not a column of Post(posts)`, `table column 'writer.name': Post(posts) has no relation 'writer'`,
  `relationship 'x' is not a relation of Post(posts)`, `CommentsRelationManager: Post(posts) has no relation 'x'`. A hely a `fájl L<sor>`.
- **Nem jelez**: nincs migráció / modell (oszlop-ellenőrzés), `->statePath()`, `->dehydrated(false)`, `Placeholder` / `ViewField`, `->relationship()` mező (csak a reláció-név),
  `getStateUsing()` / `state()` / `ViewColumn` oszlop, `*_count` / `*_exists` / `*_sum_x` oszlop, saját `->query()`-s tábla, `statePath` / `relationship` szekció gyerekei, olyan modell,
  amelyben ismeretlen trait van (a `HasFactory`, `Notifiable`, `SoftDeletes`, `HasApiTokens`, `Prunable`, `MassPrunable`, `HasUuids`, `HasUlids`, `TwoFactorAuthenticatable` ártalmatlan) vagy `__call` / `__get`.
- Lábléc (`core::diag`): `fi-field`, `fi-column`, `fi-filter`, `fi-action` (futásidejű név), `fi-page` (nem literál `::route(..)`), `fi-panel` (nem literál `->path(..)` / `->discoverResources`).

## 3. A `filament` nézet
```
filament: resources 2, pages 1, widgets 1, panels 1, warnings 2
PostResource  app/Filament/Resources/PostResource.php → Post(posts)
  pages: index /admin/posts · create · edit /{record}/edit
  form: title*, slug*, body, tilte, reading_time, author_id* (author), published_at, meta
  table: title, author.name, status · filters: status · actions: edit, delete, publish · bulk: delete
  relations: CommentsRelationManager, ReviewsRelationManager
  ⚠ form field 'tilte' is not a column of Post(posts)  app/Filament/Resources/PostResource.php L41
page Settings  app/Filament/Pages/Settings.php · /admin/settings · view filament.pages.settings
widget StatsOverview (stats)  app/Filament/Widgets/StatsOverview.php · models: Post(posts), Comment(comments)
panel admin /admin  app/Providers/Filament/AdminPanelProvider.php · resources: PostResource, UserResource
? 1 skipped: form field name is a run-time value (app/Filament/Resources/UserResource.php L26)
```
A listák 8 névnél `+N`-nel zárulnak, az egy resource-hoz tartozó `⚠` legfeljebb 3 sor. A `create` oldal útvonala, ha egyezik a kulccsal (`/create`), nincs kiírva.

## 4. `trace filament:posts`
Cél: `filament:<slug>`, `PostResource`, `filament:Post` (a modell neve), a resource fájlja. Sorok: `filament PostResource  fájl:sor`, `model Post(posts)  app/Models/Post.php`,
`panel admin /admin  fájl:sor`, `pages index /admin/posts (ListPosts), create …`, `form …`, `table …`, a szűrők / akciók, `relations CommentsRelationManager → comments`, a `⚠` sorok.
Ismeretlen cél: a „did you mean” lista a `filament:<slug>` nevekkel is bővül.

## 5. A gráf-kulcs
A resource-osztály gráfjába egy `framework_use` hívás kerül (`kind: resource`, cél = `resolved` = `filament::resource:Post`), a `$model` sorában; így a
`--grep filament::resource:Post` a modell felől is megtalálja a resource-ot. Gráf-formátum: 16.

## 6. Nincs lefedve
Filament 4 `Schema` osztályok (`form(Schema $schema)` a `->components([..])`-pel olvasható, de a külön osztályba kiszervezett séma nem), `static function formSchema()`-ba tett mezők,
`discoverPages` / `discoverWidgets` (az összes megtalált osztály listázott), config-ból jövő panel-útvonal, multi-tenancy, a relation managerek saját form / table sémája,
a szűrők nevének ellenőrzése, az outline fejlécsor a modell oldalán.
