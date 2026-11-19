# LIVEWIRE — komponens ↔ sablon ↔ használat ↔ események

Modul: `src/packages/livewire/` (`scan.rs`: szövegszkennerek, `render.rs`: a kimenet szövegei, `events.rs`: az esemény-kulcs a
gráfban). Nézet: `--format livewire` (MCP: `framework_view`, `view: "livewire"`), az outline fejlécsorai, és a
`trace livewire:counter`. A projekt-szintű olvasás egyszer fut lekérdezésenként, és csak akkor, ha a kimenet kéri (a
`livewire` nézet, vagy legfeljebb 20 komponens-osztály / sablon outline-ja); komponens-mappa (`app/Livewire`,
`app/Http/Livewire`) nélküli projektben nem fut. Hibatűrő és konzervatív: találathoz statikus név és teljesen látható osztály kell
(nincs trait, nincs `__call` / `__get`); ami futásidőben épül, a lábléc számlálójába kerül, soha nem figyelmeztetés.

## 1. Komponens ↔ sablon ↔ használat
- **Komponens**: `class X extends Component` (`Livewire\Component`, importtal vagy kiírva) bármelyik PHP-fájlban; a név a `Livewire`
  mappa alatti útból (`app/Livewire/Admin/UserTable.php` → `admin.user-table`). A `Livewire\Form` leszármazott nem komponens, hanem a
  szülő `public XForm $form` property-jének tartalma.
- **Sablon**: a `render()` törzsének első `view('livewire.x')` literálja (`resources/views/livewire/x.blade.php`); `render()` nélkül az alapértelmezett
  `resources/views/livewire/<kebab>.blade.php` (`Admin\UserTable` → `livewire/admin/user-table.blade.php`). Nem literál nézet-név (inline Blade,
  változó, `csomag::nézet`) nem köt, és nem is hiányzó. Literál nézet-név hiányzó fájlra: `⚠ view 'x' not found` (csak ha a projektnek van sablonja).
- **Használat**: `<livewire:x>`, `<livewire:dir.x>`, `@livewire('x')`, `@livewire(X::class)`, `Route::get('/x', X::class)` a `routes/*.php`-ban (a `use` sorokon át
  feloldva; a route csoport-előtagja nincs benne, az uri úgy, ahogy a hívásban áll). `<livewire:is>` / `dynamic-component` / `@livewire($név)`: lábléc.
- A Blade-frontend `@livewire('x')`-t is `component_use` hívásnak veszi (mint a `<livewire:x>` címkét), így a `--grep app/Livewire/Counter.php` a sablont is megtalálja.

## 2. Sablon ↔ osztály szerződés
- `wire:model[.live|.blur|.debounce.300ms…]="prop"` ↔ az osztály publikus property-je; `form.title` esetén a Form-objektum `title` property-je is.
  `items.0.name` / `items[0]`: az első szegmens számít. `$parent.x` és a Blade-dal épített érték kimarad.
- `wire:click|submit|keydown|change|input|blur|poll|init|…="metódus(args)"` ↔ publikus metódus (a név kis-nagybetű-érzéketlen, mint a PHP-ban).
  A `remove({{ $id }})` neve `remove`. Nem ellenőrzött: a `$`-jelű varázs-akciók (`$refresh`, `$set`, `$toggle`, `$dispatch`, `$dispatchTo`, `$parent.x()`, `$emit`…),
  a JS-kifejezés (`count++`), az `a; b`, és a Blade-dal épített név (`{{ $action }}`).
- Figyelmeztetés: `⚠ wire:click: no public method 'dec' in Counter  <sablon> L6`, `⚠ wire:model: no public property 'cnt' in Counter …`. A védett / privát tag nem cél.
  Trait-es (pl. `WithPagination`) vagy `__call`-os osztályra nincs figyelmeztetés (a tagok nem láthatók végig).
- Nem ellenőrzött: `{{ $prop }}` / `$this->prop` a sablonban; a sablonba `@include`-olt részlet; a szülőtől átadott paraméterek (`<livewire:x :count="1">`).

## 3. Események
- Küldők: `$this->dispatch('x')`, `dispatchTo('komp', 'x')`, `dispatchSelf`, v2 `emit` / `emitTo` / `emitUp` / `emitSelf`; sablonban `$dispatch('x')`, `$dispatchTo`, `$wire.dispatch('x')`,
  `Livewire.dispatch('x')`, `wire:click="$dispatch('x')"`. Figyelők: `#[On('x')]` (több is lehet), v2 `$listeners = ['x' => 'metódus', 'y']`, kliens-oldalon `Livewire.on('x')`.
  Az `echo:` csatorna-figyelő nem Livewire-esemény. Dinamikus név (`'flash-' . $kind`, `#[On('room.{id}')]`, `getListeners()`): lábléc.
- **Egy kulcs: `livewire::event:x`.** A komponens-osztály és a (Livewire-t említő) Blade-sablon gráfjába minden küldő / figyelő hely egy `framework_use` hívásként kerül
  (`kind: event`, cél = `resolved` = a kulcs), a küldő vagy a figyelő metódusában; így a `--grep livewire::event:post-saved` a küldőt és a figyelőt is megtalálja (az outline a metódus
  alatt `framework_use: livewire::event:post-saved`-ot mutat). A sima Alpine `$dispatch` (Livewire-t nem említő sablonban) nem kap kulcsot. Gráf-formátum: 15.

## 4. A `livewire` nézet
```
livewire: 6 components, 7 uses, 3 warnings
Counter  app/Livewire/Counter.php → livewire/counter.blade.php · used: 1
  props: count, label · actions: increment, save
  events: dispatches counter-changed, post-saved, counter-reset
  rules: count*
  ⚠ wire:click: no public method 'dec' in Counter  resources/views/livewire/counter.blade.php L6
Dashboard  app/Livewire/Dashboard.php → livewire/dashboard.blade.php · used: 1 · route GET /dashboard
? 1 skipped: event name is a run-time value (app/Livewire/CreatePost.php L24)
```
Komponensenként legfeljebb 4 sor (+ legfeljebb 3 `⚠` sor és `+N`). `props`: publikus property-k (`(url)` = `#[Url]`); `actions`: publikus, nem életciklus-, nem `#[Computed]` metódusok;
`events`: `dispatches` (osztály + sablon) · `listens`; `rules`: `#[Validate('required|min:3')]` / `#[Validate([...])]`, `rules()`, v2 `$rules`, `$this->validate([...])` mezői (`*` = `required`),
a Form-objektumé `form.title*` alakban. Hatókör: a lekérdezett fájlokhoz tartozó komponensek (az osztály, a sablonja vagy a kiszolgáló route-fájl). Üres: `(no Livewire components found)`.

## 5. Outline-fejlécsorok
- Komponens-osztály: `view: resources/views/livewire/counter.blade.php`, `used in: a.blade.php L3, routes/web.php L9, +N`.
- A komponens sablonja: `livewire: Counter (app/Livewire/Counter.php)`, `calls: increment L5, save L12` (a sablonból hívott létező metódusok), `binds: label, cnt`, és a `⚠` sorok.

## 6. `trace`
`trace livewire:counter` / `trace Counter` / `trace livewire:admin.user-table` (a Laravel-route / controller-találat az erősebb): `livewire Counter  fájl:sor`, `route GET /x  name=x  fájl:sor`
(teljes oldalas komponens), `view`, `used in N: …`, `props`, `actions`, `dispatches`, `listens`, `rules`, `models Post(posts)` (a modell-feloldás ugyanaz, mint a `trace` többi részén),
`⚠` sorok. Ismeretlen cél: a legközelebbi `livewire:…` nevek.

## Nincs benne
- **Volt** (`new class extends Component` a Blade-fájlban): nem olvassa; a Volt-projekt (nincs `app/Livewire`) nem kap Livewire-sorokat.
- A `Livewire::component('alias', X::class)` regisztráció és az egyedi nézet-útvonal (`livewire.view_path` config).
- A `{{ $prop }}` / `$this->prop` ellenőrzése, a gyerek komponensnek átadott paraméterek (`mount` ↔ attribútum), a `#[Modelable]` / `#[Reactive]` kötések.
- Az esemény-figyelmeztetés (küldve, de senki sem figyeli): a böngészőbeli (`@x.window`, `x-on:x.window`) figyelők miatt hamis jelzés lenne.
