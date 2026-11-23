# Változásnapló

A callgraph minden fontos változása. A munka csomagokban gyűlik a `develop` ágon, és együtt kerül kiadásra.
Formátum: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), verziók: [SemVer](https://semver.org/).
Angol változat: [CHANGELOG.md](CHANGELOG.md) — a kettő tartalma azonos.

## [Unreleased]

## [1.2.1] — 2026-11-23

### Javítás

- **Az 1.2.0 tartalma most már letölthető:** az 1.2.0 nem jelent meg letölthető kiadásként, mert a Windows-buildje
  elakadt. Az 1.2.1 az első kiadás, amely mindent tartalmaz, ami az 1.2.0 alatt szerepel; más nem változott.

## [1.2.0] — 2026-11-23

### Új

- **`trace <cél>`: egy route vagy oldal teljes lánca egyetlen rövid válaszban:** a
  `callgraph.sh trace route::events.show` legfeljebb 30 sorban kiírja a route-ot (ige, URI, név, fájl és sor), a
  middleware-eket, a controller-akciót, a FormRequest- vagy inline szabályokat (`*` = kötelező), a modelleket a
  táblájukkal, az Inertia-oldalt a controller által átadott props-szal, és az oldal által használt komponenseket. A
  cél lehet route-név (`route::x`), `route:GET:/uri`, URI, `Controller::method` vagy `Pages/X.vue`. JS-projektben a
  szerver-route a kliensoldali hívóival együtt jelenik meg (a regex-route-okat az út alapján illeszti). Ismeretlen
  célra egy sort ad, legfeljebb 5 legközelebbi névvel. MCP-n ez az új `trace` eszköz.
- **A `--format routes` a JS / TS route-táblákat is olvassa:** a vue-router táblák `/chats/:id →
  src/pages/ChatPage.vue (chat)` alakban jelennek meg, a beágyazott `children` utakat összefűzve, a lusta `import()`
  komponenseket feloldva, a `redirect /x` / `redirect name:x` átirányításokkal. A Hono, Express és itty
  szerver-route-ok (`app.get('/x', h)`, az `app.route('/p', sub)` / `app.use('/p', router)` csatolás és a
  `route("GET", /re/, handler)` segédfüggvény) az importokon át feloldott kezelővel, `←` után pedig azokkal a kliens
  `fetch` / `axios.*` / `ofetch` / `new WebSocket` hívásokkal jelennek meg, amelyek metódusa és útja illeszkedik. A
  `router.push({ name })`, `router.replace` vagy `<RouterLink :to="{ name }">` olyan route-neve, amelyet egy tábla sem
  definiál, `⚠ unknown route name 'x'` jelzést kap; az ismert szerver-útvonal alá eső, de egyetlen szerver-route-ra
  sem illeszkedő kliens-URL pedig `⚠ no server route: GET /api/x` jelzést.
- **`--format props`: mit adnak át a szülők a Vue-komponenseknek:** mappára vagy fájlra futtatva listázza a
  `⚠ missing required prop 'title'  Parent.vue L12 <Card>`, `⚠ unknown prop 'colr'` és `⚠ unknown slot 'foot'`
  eseteket. Szándékosan engedékeny: a `v-model` / `v-model:x` átadásnak számít, a kebab-case egyenlő a camelCase-szel,
  nincs „ismeretlen”, ha a gyerek `$attrs` / `useAttrs` / `inheritAttrs` / `mixins` elemet vagy importált, illetve
  összetett props-típust használ, nincs „hiányzó”, ha a szülő `v-bind="obj"`-ot terít, és a szabványos
  HTML-attribútumokat (`id`, `title`, `data-*`, `aria-*`, űrlap- és input-attribútumok, …) soha nem jelzi. Az
  `unplugin-vue-components` mellett az import nélküli tag az egyetlen azonos nevű `.vue` fájlhoz kötődik, `(auto)`
  jelöléssel.
- **A Vue-komponens szerződésének több része látszik az outline-ban:** a `slots (N)` a `<slot>` kimeneteket, az
  `exposes (N)` a `defineExpose` neveit sorolja; a `provide('x')` és az `inject('x')` egy kulcson osztozik, így a
  `--grep 'vue::provide:theme'` mindkét oldalt mutatja; az importált interface-re vagy típusra hivatkozó
  `defineProps<Props>()` props-ai is megjelennek. A Pinia `defineStore('id', …)` egy `store` blokk (az actionök és
  getterek metódusok, a `state` kulcsai tulajdonságok; setup-store-nál a visszaadott függvények), és a
  `useXStore().act()` vagy a `const s = useXStore(); s.act()` hívás rá mutat, a sablonban is.
- **Tailwind 4 a stíluslapokban és a `--format classes` nézetben:** az `@utility` osztályok és a `@theme` tokenek
  utility-jei (`--color-brand-500` → `bg-brand-500`, `text-brand-500`, …) definíciónak számítanak, az `@apply`
  osztályai pedig használatnak. A nem létező tokenre épülő, használt osztályt jelzi, például
  `theme: no such token text-brand-450 A.vue:9 (like text-brand-500, …)`; csak akkor, ha a projekt más `brand-…`
  tokeneket definiál, a Tailwind saját neveinél és `@theme` nélküli projektben soha.
- **Blade: hiányzó nézetek, egyedi direktívák és `@vite` belépők:** az outline fejléce jelzi azt az `@include`-ot vagy
  `<x-…>`-et, amelynek nézete nem létezik (`⚠ missing view: partials.gone (@include L4)`; csak statikus névnél, a
  csomag-nézetet, mint a `mail::x`, soha), futásidejű célként jelöli a `<x-dynamic-component>`-et, és a használt
  `Blade::directive` / `Blade::if` direktívát a regisztrációjához köti
  (`directives: @datetime → app/Providers/AppServiceProvider.php L12`; a provider outline-ja direktívánként
  listázza az őket használó nézeteket). Az `@vite([...])` fájljai import-élek lesznek, így a
  `--grep resources/js/app.ts` megtalálja a sablont.
- **A HTML belépőfájlok a szkriptjeikre és stíluslapjaikra hivatkoznak:** a `<script src>` és a
  `<link rel="stylesheet" href>` `import /src/main.ts` sorként jelenik meg és importnak számít, így a
  `--grep src/main.ts` megtalálja az `index.html`-t. A külső URL-eket figyelmen kívül hagyja.
- **A React függvény-komponensek props-ai megjelennek az outline-ban:** a komponens alatt
  `props (4): title*, size, onSave*, children` sor áll (`*` = kötelező). Az első paraméterből olvassa őket
  (destrukturálás alapértékkel és `...rest`-tel), a TS-típusából (inline, helyi vagy importált `interface` / `type`,
  `extends`, `A & B`), a `FC<Props>`, `memo(…)` / `forwardRef<El, Props>(…)` alakból, az `X.propTypes`
  (`.isRequired`) és az `X.defaultProps` objektumból. A projekt saját hookja (React-hookot futtató
  `function useCart()`) `[hook]` jelölést kap, és a hívói rá mutatnak; a `createContext`, a `<X.Provider>` /
  `<X.Consumer>` és a `useContext(X)` egy kulcson osztozik (`react::context:<fájl>::X`), így a `--grep` a providert
  és minden fogyasztót megtalál.
- **A `--format props` a React JSX-használatokat is ellenőrzi:** listázza a
  `⚠ missing required prop 'onSave'  src/pages/Home.tsx L19 <Card>` és a `⚠ unknown prop 'colour'` eseteket.
  Óvatos: a `{...spread}` kikapcsolja a „hiányzó”, a `...rest`, a `HTMLAttributes`-t kiterjesztő, unió vagy fel nem
  oldható props-típus az „ismeretlen” ellenőrzést, a `key`, `ref`, `children`, `className`, `style`, `data-*` és
  `aria-*` pedig sosem ismeretlen. Csak a statikusan kötött komponenst nézi (ugyanabban a fájlban vagy egy
  projektfájl közvetlen importjaként).
- **A `--format routes` a react-routert is olvassa:** a `<Routes><Route path element /></Routes>` fákat (beágyazva,
  `index`-szel) és a `createBrowserRouter([…])` / `createHashRouter` / `useRoutes([…])` tömböket (`element`,
  `Component`, `lazy`, `children`) listázza, és a literális `<Link to>` / `<NavLink>` / `<Navigate>` /
  `navigate('/x')` útvonalat, amelyre egy route sem illeszkedik, `⚠ unknown route path '/x'` jelzéssel látja el (a
  `:param` és a `*` illeszkedik). Route-tábla nélkül vagy nem literális útvonalnál nincs figyelmeztetés.
- **Reactban írt Inertia-oldalak:** `@inertiajs/react` projektben az `Inertia::render('Events/Show', …)` a
  `Pages/Events/Show.tsx` (vagy `.jsx`) fájlra oldódik, az oldal props-ai az alapértelmezetten exportált komponens
  props-ai, így a `--format inertia` jelzi a controller által soha át nem adott kötelező props-ot, a `trace` pedig
  eljut az oldalig és az általa importált komponensekig.
- **A `gen react:` React-komponenst, a `gen hook:` saját hookot ír:** a
  `react:Card>props(title:str,size?:'sm'|'md'='md')+ref:count:int=0+fn:inc():void{…}+jsx>div.card>h1{{ title }}` a
  `--write src/components/Card.tsx` mellett függvény-komponenst ír: `.tsx`-ben típusos props-interface-szel,
  `.jsx`-ben sima JS-sel és JSDoc-kal; a `ref:` `useState`, az `emits(save:int)` `onSave` callback prop, az
  `on:mounted{…}` `useEffect`, a `form(a,b)` Inertia `useForm` lesz. A pontosvessző, a behúzás és a default vagy
  névvel exportálás a projekt komponenseit követi. A `--into src/Card.tsx` props-ot, állapotot, kezelőket és elemeket
  ad egy meglévő komponenshez; a `--replace` `.tsx` / `.jsx` fájlra nem érhető el, és ezt egyértelmű hibával jelzi.
  A `hook:useCart>ref:items:list<Item>=[]+fn:add(…){…}` a `--write src/hooks/useCart.ts` mellett hook-modult ír. Az
  MCP `generate` eszköz mindkét alakot elfogadja.
- **A `laravel:` preset Inertia + React projektben React-oldalakat ír:** `@inertiajs/react` mellett az `Index`, a
  `Create`, az `Edit` és a `Show` oldalt (például `Pages/Fees/Create.tsx`) `.tsx` vagy `.jsx` fájlként írja meg, a
  projekt saját input-, label-, hiba- és gombkomponenseivel (név és props alapján megtalálva, vagy a
  `.callgraph/gen.toml` `[components]` szakaszában megadva). Fordítás-hívás csak akkor kerül bele, ha a meglévő
  oldalak már használnak fordító hookot, például `useTranslation()`-t.
- **Livewire-komponensek: osztály, nézet és használat összekötve, a `wire:` nevek ellenőrizve:** a
  komponensosztály a Blade-nézetéhez (a `render()` alapján vagy az alapértelmezett elnevezéssel) és a használati
  helyeihez kötődik (`<livewire:counter>`, `@livewire('counter')`, teljes oldalas `Route::get(…, Counter::class)`).
  A nézetben a `wire:model` tulajdonságokat és a `wire:click` / `wire:submit` metódusokat az osztályhoz és a Form
  objektumaihoz méri, a hiányzó publikus metódust vagy tulajdonságot `⚠` jelzi (csak statikus névnél; a mágikus
  `$refresh`, `$set`, `$toggle` és `$dispatch` sosem kap jelzést). A `$this->dispatch('x')`, a Blade
  `$dispatch('x')`, az `#[On('x')]` és a v2-es `$listeners` egy kulcson osztozik (`livewire::event:x`), így a
  `--grep` a küldőt és a figyelőt is megtalálja. Az új `--format livewire` nézet (MCP-n framework-nézetként is)
  komponensenként listázza a nézetet, a használatokat, a route-ot, a tulajdonságokat, az akciókat, az eseményeket, az
  `#[Validate]` / `rules()` validációs szabályait és a figyelmeztetéseket, a végén egy sorral a futásidőben épített
  nevekről; a `<livewire:is :component="$x">` itt jelenik meg, nem számít használatnak. A `trace Counter` és a
  `trace livewire:counter` a komponens láncát mutatja. A Volt-komponenseket még nem támogatja.
- **Filament admin panelek:** a resource a `$model` modelljével, a `getPages()` oldalaival (a panel útja alatti
  URI-kkal), a `form()` mezőivel (a `Section`, `Grid` és `Tabs` belsejében is), a `table()` oszlopaival,
  szűrőivel, akcióival és tömeges akcióival, valamint a `getRelations()` relation managereivel jelenik meg; az
  önálló oldalak, a widgetek (a lekérdezett modellekkel) és a panel providerek (id, út, resource-ok) is
  szerepelnek. Az ellenőrzés óvatos: azt az űrlapmezőt vagy táblaoszlopot, amely nem oszlopa, fillable-je, castja,
  accessora vagy relációja a modellnek, csak akkor jelzi, ha a modell migrációja megvan, és jelzi azt a relation
  managert vagy `->relationship('x')` hívást, amelynek relációja hiányzik a modellből. Az új `--format filament`
  nézet (MCP-n framework-nézetként is) egy sorral zárul a futásidőben épített nevekről; a `trace PostResource`, a
  `trace filament:posts` vagy a modell neve a láncot mutatja, a `--grep filament::resource:Post` pedig megtalálja a
  resource-ot. A Filament 4 Schema osztályait még nem támogatja. A cache formátuma a Livewire és a Filament miatt
  ismét változott; a meglévő cache magától újraépül.
- **Több nézet is megmondja, mit hagyott ki:** a `--format forms` mintájára a `routes`, az `inertia` és a `props`
  nézet is okonként egy sorral zárul, például futásidőben épített route-név, nem literál oldalnév vagy `v-bind`
  objektum esetén (`? 2 kihagyva: <ok> (a.vue L3, b.vue L9, +N)`), ahelyett hogy csendben kihagyná az elemet.
- **A `next` javaslatai tanulhatnak a saját döntéseidből (opt-in, csak helyben):** a `CALLGRAPH_NEXT_LEARN=1` a
  `CALLGRAPH_NEXT=1` vagy a `--next` mellett a javaslatokat abból az elfogadás / visszalépés naplóból rendezi át,
  amelyet a callgraph a projekt `.callgraph/results/` mappájában vezet; semmi nem hagyja el a gépet. Csak a
  szabályokból származó javaslatokat rendezi át vagy hagyja el, újat soha nem talál ki. Kevés adatnál a sorrend
  pontosan a szabályok szerinti; az a javaslatfajta, amelyet gyakran visszalépés követ, hátrébb kerül vagy kimarad,
  és ha egyik sem elég biztos, nem jelenik meg `next` sor. Alapból ki van kapcsolva; a változó törlésével
  kikapcsolható. Az új `callgraph next --eval` visszajátssza a helyi naplót, és megmutatja, hogyan teljesített volna
  a tanult sorrend a szabályok sorrendjéhez képest (top-1 találati arány, átlagos reciprok rang, visszalépési
  arány); kevés adatnál ezt jelzi is.
- **A `CALLGRAPH_COUNT=1` a `DO_NOT_TRACK` ellenére visszakapcsolja a névtelen napi számlálást:** a `DO_NOT_TRACK`-et
  más eszközök is figyelik, ezzel a kifejezett opt-innel a callgraph napi számlálása mégis futhat. A
  `CALLGRAPH_TELEMETRY=0`, a `CALLGRAPH_UPDATE_CHECK=0` és a CI továbbra is kikapcsolja, a `CALLGRAPH_COUNT=0`
  kikapcsolva tartja, az opt-in telemetriát pedig nem érinti (`DO_NOT_TRACK` mellett az ki marad).

### Változás

- **A Vue emit-kulcsok a komponensükhöz tartoznak:** egy emit mostantól `Card.vue::emit:save`, nem `vue::emit:save`,
  és a szülő `<Card>`-on lévő `@save="h"` / `v-on:save` figyelője ugyanerre a kulcsra mutató él, így a
  `--grep 'Card.vue::emit:save'` az emittáló kódot és a figyelő szülőket is mutatja. A natív DOM-események és a
  dinamikus `@[ev]` nem kapnak élt. A cache formátuma változott; a meglévő cache a következő futáskor magától
  újraépül.
- **A Vue-sablon kifejezéseit ugyanúgy elemzi, mint a scriptet:** a `{{ }}`, `:x`, `@x`, `v-if` és `v-model`
  hívásai ugyanazt a kategóriát kapják, mint a `<script>`-ben (egy `const store = useX()` kötés, importok, beépített
  függvények), az inline kezelőket (`@click="a(); b++"`) is beolvassa, az optional chaining hívásokat (`a?.b()`)
  megtalálja, a dinamikus hívásokat (`handlers[name]()`) jelöli. A `v-for` elemei és a slot-propok helyi nevek, így
  az `item.open()` már nem jelenik meg hívásként. A nem parsolható kifejezésre a korábbi, név szerinti olvasás fut.
- **A `useContext(X)` a contextre mutat, nem a Reactra:** mostantól a `react::context:<fájl>::X` kulcsra mutató él,
  nem `react::useContext`. A cache formátuma ismét változott; a meglévő cache magától újraépül.

## [1.1.1] — 2026-11-18

### Új

- **Rövid hatásnézet a `--compact` kapcsolóval:** a `--grep … --depth N --compact` a hívókat szintenként, fájlonként
  egy sorban listázza (`path: A()  L12, B()  L40 ← target`, kb. 160 karakternél tördelve), így egy széles
  hatáselemzés rövid marad. Az útvonal-regisztrációk a tényleges kódjukat mutatják, például
  `L5 Route::delete('songs', [SongController::class, 'destroy'])->name('songs.destroy')`. Ha egy fájl összes
  bejegyzése ugyanarra a célra mutat, a `← target` csak egyszer, közvetlenül a fájlnév után áll. MCP-n a
  `find_callers` `compact: true` paramétere.
- **A `locate` sima mondatot is megért:** a feladatot úgy írhatod le, ahogy kimondanád. A gyakori angol és magyar
  töltelékszavakat (hol, a, az, hogy, …) figyelmen kívül hagyja, és a fejlécben `ignored: …` alatt kiírja; az
  idézőjeles kifejezéseket továbbra is kifejezésként keresi. A fordítófájlokban (Laravel lang fájlok, JSON/TS
  szótárak, `.po` / `.xliff`) talált egyezések sokkal kisebb súlyúak, így a név- vagy szimbólumtalálatos kódfájl
  előrébb kerül. Ha a szavak többsége csak felületi szövegre illeszkedik, a fejléc az angol kódnév hozzáadását
  javasolja; ha kódban nincs találat, azt kiírja (`nincs kódtalálat`), és a lista megmarad, ami segít, ha felületi
  feliratot keresel.

### Változás

- **A `--grep` / `find_callers` pontosan a megadott nevet keresi:** az `execute` már nem találja meg az
  `executeCommand`-ot, a `store` (metódus) pedig a `Store` osztályt. Az osztálynév továbbra is megtalálja a minősített
  hívásait (a `SongService` az `App\Services\SongService::x`-et). A régi „tartalmazza” kereséshez regexet használj,
  pl. `'.*execute.*'`; a regex és a `Type::method` minták továbbra is kis- és nagybetűre nem érzékenyek.
- **Tisztább híváslisták Go-hoz és Rusthoz:** a Go outline a metódus saját objektumán lévő hívást sima
  metódusnévként mutatja (mint a PHP `$this->x`), a többit `Type::method` alakban. Rustban az egymásba ágyazott
  mezőkön át történő hívás (`self.a.b.m()`) a `--depth`-ben már nem számít biztos hívónak, csak a `--guess`
  kapcsolóval jelenik meg.
- **Az `update` / `callgraph.sh update [--dev]` figyelmeztet az MCP újracsatlakoztatására:** sikeres frissítés után
  kiír egy megjegyzést, hogy a futó MCP-szerver a régi verziót használja, amíg újra nem csatlakoztatod (Claude Code:
  `/mcp`, majd Reconnect).

### Javítás

- **A `--grep Type::method` és a `--depth N` olyan hívókat is megtalál, amelyeket korábban kihagyott:** a Python
  `self.` / `cls.` hívásait, Go-ban pedig a metódus saját objektumán, típusos paramétereken, típusos lokális
  változókon (többértékű értékadásnál is) és metódusláncokon keresztüli hívásokat. Korábban az eredmény azt jelezhette,
  hogy a metódust senki sem hívja, vagy Pythonban a 2. szintű lista üresen jöhetett vissza.
- **A `--depth` az akció minden útvonalát felsorolja:** egy több `[Controller::class, 'method']` útvonalat tartalmazó
  routes fájlnál korábban csak az első látszott a 2. szinten, és két, ugyanarra az akcióra mutató útvonalból is csak
  egy; most mind. A `--grep 'Ctrl::m' --depth N` magára a controller-akcióra az útvonal-regisztrációkat az 1. szinten
  listázza; korábban azt írta, hogy nincs hívó. Ha egy rövid `Ctrl::method` minta több névtérben is talál azonos nevű
  osztályt, az 1. szinten minden útvonalsor a teljes nevű célt nevezi meg, a normál és a `--compact` nézetben is.

## [1.1.0] — 2026-11-17

### Új

- **A Blade sablonok (`.blade.php`) önálló nyelvként elemződnek** (korábban üres eredmény): fájlonként egy nézet,
  `admin.users.index` formájú névvel, a szekciók, push-ok és slotok sortartománnyal; az `@extends` / `@include` /
  `@each` / `<x-…>` / `<livewire:…>` a nézetfájlokra oldódik fel, így egy layout hívói között megjelennek a rá
  épülő nézetek; a `@props` property-ként látszik; a `{{ }}`, `{!! !!}`, `@php` és a direktíva-argumentumok PHP-ja
  is elemződik; az `@if` / `@foreach` … vezérlési ágként jelenik meg; a `route('x')` a Laravel route-ra mutat
  (`→ POST /users · Controller@method`, ismeretlen név jelölve); a `__()` / `@lang` részt vesz az i18n-ellenőrzésben;
  a „rendered by” sor pedig a controllerek `view('…')` hívásait mutatja.
- **MCP `undo` eszköz:** visszaállítja a `generate` vagy a `rename` utolsó változtatását — fájl megadásakor azt az egy
  fájlt, egyébként az utolsó változtatáscsoport egészét. Az előzmény többlépéses: minden hívás egy változtatással
  tovább lép vissza (korábban két egymás utáni undo az utolsó változtatást ide-oda kapcsolgatta). A `generate`-hez és
  a `rename`-hez hasonlóan csak akkor létezik, ha a szerver `--generate` / `CALLGRAPH_GENERATE=1` móddal fut, és
  `readOnlyHint: false` jelölést kap.
- **`rename term <régi> <új>`: egy teljes fogalom átnevezése minden írásmódjában:** a `rename term post article`
  a PHP-, JS-, TS- és Vue-kódban mindenhol lecseréli a szót, ahol egész szóként szerepel, a kis- és nagybetűt és a
  többes számot megtartva: `Post` / `post` / `posts`, `post_id`, `StorePostRequest`, `/posts/{post}`,
  `admin.posts.index`, `Posts/Index.vue`. A `-s`, `-es` és `-ies` többes számot magától képzi, a szabálytalant
  `person/people` alakban lehet megadni. Az útvonalukban a szót tartalmazó fájlok átkerülnek az új helyre, és ha a
  migrációk a szóval hoznak létre táblát (pivot táblát is) vagy `*_id` oszlopot, egy új, visszafordítható migráció
  nevezi át őket (`rename_post_to_article`); a meglévő migrációk érintetlenek. Kimarad: a csupa nagybetűs szó
  (`POST`), a HTTP-igés hívás (`->post(`, `Route::post(`, `postJson(`), a `vendor` és a `node_modules`. Az
  érték-helyzetű, idézőjeles szót, amely HTTP-ige is lehet (`'method' => 'post'`), csak jelzi, nem módosítja. A
  `--dry` a teljes tervet mutatja (áthelyezett fájlok, fájlonkénti változások, jelzett sztringek, a migráció), és
  egyetlen `undo` mindent visszavon, az új migrációt is. MCP-n a `rename` eszköz `term` fajtája.
- **`gen` előnézetek és további elhelyezési lehetőségek:** `--into` / `--replace` mellett a `--dry` először kiírja,
  hová kerülne a kód, pl. `→ app/Pay.php::Pay after charge (L41)` vagy `→ f.vue: script L12-14`, a `--diff` pedig a
  `--replace`-t rövid unified diffként mutatja, semmit nem ír. PHP `--into` mellett a `--before <metódus>` és az
  `--at L<n>` egy metódus elé vagy egy sorba teszi a kódot (az osztálytörzsön kívüli, pl. a `{` sorára mutató `--at`
  azonnal hibát ad, az üzenet megadja a törzs sortartományát), az `--upsert` pedig egy létező metódus `--into`-ját
  cserévé alakítja (`--expect` mellett az elvárást akkor ellenőrzi, ha a tag már létezik). A Vue
  `--into fajl.vue::template#id` (vagy `::#id`, `::.class`) az adott template-elem belsejébe fűz, a kifejezést
  sablon-markupként olvasva, a `--replace fajl.vue::script` / `::template` / `::style` pedig az egész szakasz
  tartalmát cseréli (a tagek maradnak), elavulható sortartomány nélkül. Az MCP `generate` eszköz megkapta a `before`,
  `at`, `upsert` és `diff` paramétert.
- **`gen` PHP: traitek, névtelen osztályok és Laravel-osztálynevek:** egy típus alatti `use:HasFactory,Notifiable`
  az osztálytörzsbe `use HasFactory, Notifiable;` sort ír (`c:Post<Model> > use:HasFactory + +comments():HasMany{…}`).
  Az `a:<Migration> > +up():void{…}` névtelen osztályt ír, `return new class extends Migration { … };`, ahogy a
  Laravel migrációkban szokás. Laravel-projektben (`laravel/framework` a `composer.json`-ban) a gyakori
  framework-osztályokat, pl. `Model`, `Schema`, `Blueprint`, `Route`, `Request`, `Rule`, `Builder`, `HasMany` vagy
  `RefreshDatabase`, `use:` nélkül is importálja; a több csomagban is létező nevekhez (pl. `Response`) továbbra is
  `use:` kell.
- **`gen` PHP-utasítások osztályon kívül: `php{ … }` és `raw{ … }`:** a `php{ Route::get('/posts',
  [PostController::class, 'index']); }` sima PHP-kódot ír. `--write routes/x.php` mellett létrehozza a fájlt (a
  `<?php` sorral és az importokkal), `--into routes/web.php` mellett a fájl végére fűzi, vagy `--at L<n>` /
  `--after L<n>` helyre teszi, `--replace bootstrap/app.php:L14-L16` mellett pedig azt a sortartományt vagy egy
  metódust cseréli. A kód átveszi annak a sornak a behúzását, ahová kerül vagy amelyet cserél, és egy üres `php{}`
  `--replace fajl:La-Lb` mellett törli a sorokat. A `raw{ … }` a szöveget pontosan úgy írja ki, ahogy megadtad, pl.
  egy Blade-nézetet `--write resources/views/app.blade.php` mellett; a Blade-fájlokon nincs PHP-szintaxisellenőrzés.
- **Vue prop-ok interfészből, alapértékekkel és `defineModel`-lel:** a `defineProps<Props>()` mostantól feloldja a
  helyi `interface`-t vagy típusaliast (az `extends`-et és az `A & B`-t is); a `withDefaults` alapértékei
  `type $name = default` alakban látszanak, és a prop nem számít kötelezőnek; a `defineModel` `model:` sorként jelenik
  meg a vázlatban, a `--format api`-ban pedig `model` szakaszként. A csak típust használó `defineEmits` is a valódi
  sorát mutatja.
- **Kotlin-támogatás (`.kt`, `.kts`):** osztályok, data/sealed/enum osztályok, objektumok és companion objectek,
  interfészek, felső szintű és extension függvények, `suspend` és `@Composable` függvények, property-k (a `by lazy` és
  a `by viewModels()` is), hívások fogadótípus-feloldással, `::ref` / `Type::name` hivatkozások és vezérlési ágak. A
  kategóriák lefedik a Compose-t (nézetek, modifierek, API), a coroutine-okat, az Androidot és a Kotlin standard
  könyvtárat; a `*.gradle.kts` fájlok Gradle build scriptként vannak jelölve. A `hiltViewModel()`, a `viewModel()` és
  a `by viewModels()` injektálási él, a view modelen hívott metódusok pedig a típusára oldódnak fel. A Hilt `@Inject
  constructor`, `@Provides` és `@Binds` megmutatja, mi injektálódik és melyik metódus szolgáltat egy típust. A
  `NavHost` `composable("route") { Screen() }` és a `navController.navigate("…")` navigációs él (a típusbiztos
  útvonalak is: `navigate(Detail(…))` / `composable<Detail>`), akkor is, ha a `NavHost` másik fájlban van. Az
  interfészmetódus és a megvalósításai hívói látják egymást. Az Android-projektek profilt kapnak (ui / view model /
  data / domain / di rétegek; a `*.gradle.kts` nem számít alkalmazáskódnak), a Kotlin pedig megjelenik a használati
  statisztikában. A `!!` nem rejti el a hívás fogadóját, a `*Test.kt` fájlok és az `androidTest` mappák a `locate`-ban
  tesztnek számítanak, a metódustörzsek szűrése pedig helyes (kommentek, raw stringek).
- **Swift: callback-ek, protokollok és navigáció:** a callback-argumentumként átadott metódushivatkozás
  (`Composer(onSend: vm.send)`) a hivatkozott metódus hívása. A callback-property-ben tárolt closure (`socket.onFrame
  = { … }`) össze van kötve azzal a hellyel, ahol a property-t meghívják. A protokollmetódus és a megvalósításai hívói
  látják egymást (`via`), és az `extension`-ben deklarált megfelelés is számít. A `.environment(store)` /
  `.environmentObject` injektálási él, a `navigationDestination`, `sheet`, `fullScreenCover`, `popover` vagy
  `NavigationLink` alatt épített nézetek navigációs élek. A fogadótípust a `@State`, `@Bindable`, `let x: T`,
  `@Environment(T.self)` és a paraméterek adják. A pozícionálisan (`.onChange(of: x, vm.changed)`) vagy `#selector(…)`
  útján átadott hivatkozás hivatkozás-hívás; a key path nem hívás. A SwiftData `@Query var items: [Item]` modell-él, a
  `modelContext` hívásai pedig kategorizáltak. A `navigationDestination { switch … }` minden nézete navigációs él, a
  generikus nézetek (`List<Item>(…)`) komponenshasználatnak számítanak, az actor metódusok pedig feloldódnak. A
  fájlokon átívelő property-láncon át indított hívás (`self.store.session.user.load()`) az utolsó property típusának
  metódusára oldódik fel. A tárolt closure-t meghívó metódus (`onFrame?(…)`) a closure hívottjainak közvetlen
  hívójaként jelenik meg, `via <Type>::<property>` jelöléssel; ezek a hívók és a fájlokon át feloldott hívók
  (Kotlin-útvonalak, Swift property-láncok, Rust trait-korlátok) a hívólista első szintjén szerepelnek, a „called
  through a stored closure or resolved across files” alatt.
- **`callgraph mcp --allow <mappa>`:** az MCP-szerver egy további mappát is olvashat, pl. egy testvér `../android`
  mappát. A kapcsoló ismételhető, vagy a `CALLGRAPH_MCP_ALLOW` környezeti változó használható; alapból a szerver a
  projektgyökérre korlátozott marad.
- **Rust: függvényhivatkozások és több fogadótípus:** az értékként átadott függvény vagy metódus (`.map(Self::parse)`,
  `.and_then(Foo::new)`, `.map(helper)`) hívásnak számít. A generikus korláttal (`T: Trait`, `where`) vagy a metódus
  visszatérési típusával (`self.make()?`, `Type::open().unwrap()`) típusozott fogadók feloldódnak. A ciklusváltozók
  (`for b in self.bars`), az `if let Some(x)` / `while let` / `match Some(x)` kötések és az iterátor-closure
  paraméterek (`items.iter().map(|x| x.name())`) megkapják az elemtípusukat, így a metódushívásaik feloldódnak; `T: A
  + B` esetén a metódust deklaráló trait számít, akkor is, ha a traitek más fájlokban vannak deklarálva. A `#[test]` /
  `#[tokio::test]` függvények és a `#[cfg(test)]` modulok tesztként vannak jelölve, a `Some` / `Ok` / `Err` /
  `Box::new` / `Default::default` már nem zsúfolja a híváslistát, a helyi closure hívása pedig dinamikusnak van
  jelölve. A trait-metódus és a megvalósításai hívói mindkét irányban látják egymást (`via`).
- **JS/TS: az értékként átadott függvények hívónak számítanak:** a hívás- vagy `new`-argumentumként, tömbelemként
  vagy objektum-property értékeként (rövidített alakban is) átadott függvény, pl. egy route-tábla handlere
  `guarded(api.listX)`-ben, `(ref)` jelölésű hivatkozás-él. Megjelenik a hívólistákban, számít a `--depth` /
  hatáselemzésben, és a függvény már nem tűnik hívó nélkülinek. Ez a fájl saját függvényeire, a helyi importokra, az
  objektum-literálok metódusaira és a névtér-import (`import * as api`) tagjaira vonatkozik; a csomagok függvényeire
  nem.
- **Több JS/TS import oldódik fájlra:** a tsconfig `extends` (string vagy tömb, láncolva is) és `references` mezőjét
  követi, egy `paths`-bejegyzésnek több célja és a minta közepén `*` is lehet (a leghosszabb illeszkedő előtag nyer),
  és beolvassa a `vite.config.*` / `vitest.config.*` `resolve.alias` beállítását (objektum vagy
  `[{find, replacement}]`, `fileURLToPath(new URL(…))` vagy `path.resolve(__dirname, …)` alakban). Egy aliast csak
  akkor használ, ha a célja létezik. Így az olyan importok, mint a `@shared/api` vagy a `@kit/…`, megtalálják a
  hívóikat, és a `deps` már nem jelzi őket hiányzó csomagként. A kiterjesztés nélküli import a `.tsx`, `.jsx`, `.mts`,
  `.cjs`, `.cts` fájlokat és az `index.vue` / `index.tsx` / `index.jsx` / `index.mjs` fájlt is megtalálja, TypeScript
  projektben pedig a `./a.js` az `a.ts`-re oldódik. Az importfeloldás gyorsabb is: minden mappát egyszer listáz, nem
  próbálgat végig minden lehetséges fájlt.
- **PHP: `X::class` hivatkozások:** az `X::class`, a `[X::class, 'method']` hívható pár (`X::method`) és a
  `Route::get($uri, X::class)` invokálható controllere (`X::__invoke`) `(ref)` jelölésű hivatkozás-él, a `use` és a
  névtér alapján feloldva. A `--grep X`, a `locate X` és a `--depth` mostantól az osztályt megnevező útvonalakat,
  konfigbejegyzéseket és bindingokat is megtalálja.
- **PHP: a hívások a docblock-típusokon keresztül is feloldódnak:** a csak docblockban típusozott változó vagy
  property mostantól a megfelelő metódusra vezet, így a hívói is megtalálhatók (korábban a `--grep 'Model::method'`
  ilyenkor nem talált hívót). Olvasott címkék: az osztályon lévő `@mixin` (Laravel API resource-ok: a resource-ban
  nem definiált `$this->m()` a modell `m` metódusára mutat, kivéve a resource saját segédmetódusait), az értékadás
  vagy a `foreach` fölötti `@var` (az elemtípus is, pl. `Collection<Post>` vagy `Post[]`), a `@property` /
  `@property-read`, a típus nélküli paraméter `@param`-ja, a `@return` és a `@method`. A kódban megadott típus
  elsőbbséget élvez, és csak egyetlen, egyértelmű osztályt használ (a `?Post` / `Post|null` is annak számít).
- **Teljes Laravel route-tábla a `--format routes` kimenetben:** Laravel-projektben minden útvonal egy sor: metódus,
  teljes URI, action, `name=`, `[middleware]` és `fájl:sor`; a csoportok prefixét, nevét és middleware-ét a beágyazott
  csoportok is öröklik. Ugyanez a tábla táplálja a `--format ziggy`, `inertia` és `forms` nézetet. A literál
  konfigtömbből ciklusban generált útvonalak (`foreach (config('catalogue.pages') as $name => $page)
  Route::get($page['uri'], …)->name($name)`) valódi sorokká bomlanak ki, és már nem dinamikusak: a `ziggy` ismeri a
  nevüket, az `inertia` pedig az oldalaikat (a konfigban megnevezett, de nem létező oldalkomponenst jelzi). A tárolt
  route-adat egyszer, csendben újraépül.
- **A framework által végzett Laravel-drótozás:** azok a hívások, amelyek csak a Laravelen keresztül léteznek,
  mostantól a céljukra mutatnak, így a cél hívólistája is mutatja őket: az Eloquent local scope hívása
  (`Product::query()->stocked()`, `Product::stocked()`) a `scopeStocked`-ra; az esemény kiváltása (`E::dispatch(…)`,
  `event(new E)`) a listenerére (amelyet a `handle(E $e)`, az `Event::listen` vagy a `$listen` tömb alapján talál
  meg); a `Job::dispatch(…)` a job `handle` metódusára; a `->notify(new N)` az `N::toMail`-re; a
  `Model::observe(O::class)` / `#[ObservedBy]` pedig az observer metódusaira.
- **A `--format inertia` jelzi a soha át nem adott propokat:** ha egy oldal olyan kötelező propot deklarál, amelyet
  sem render-hely, sem `HandleInertiaRequests::share` kulcs nem ad át, `! never passed: x` sort kap. A tárolt gráf
  egyszer, csendben újraépül.
- **A PHP konfig- és lang fájlok vázlata kulcs-útvonalakat mutat:** a tömböt visszaadó `config/*.php` vagy
  `lang/*/*.php` fájl a kulcsait útvonalként, sorszámmal listázza (`pages.home.uri = '/'`).
- **Rust CLI- és könyvtárprofil:** a `main.rs`, `lib.rs` vagy `bin` célt tartalmazó crate profilt kap a belépési
  pontjaival, a `src/<modul>` mappákkal mint modulrétegekkel és a felsorolt workspace-tagokkal. A `tests`, `benches`,
  `examples` és a fixture- vagy golden-minta mappák már nem teszik a projektet webes, Vue- vagy SwiftUI-projektnek
  látszóvá.
- **TypeScript interfész-tagok property-ként:** az `interface` tagjai property-ként szerepelnek, az opcionálisak
  `?`-lel.
- **A hibás hivatkozások jelölve vannak Markdownban és HTML-ben:** a hiányzó fájlra mutató relatív link, illetve a
  megfelelő címsor nélküli `#anchor` `⚠ missing` jelölést kap.
- **Zárolt verziók a `package.json` / `composer.json` vázlatában:** a függőségi kulcsok mellett látszik a zárolt
  verzió.
- **Fordítási kulcsok a `--format i18n` móddal:** egy lang fájlnál megmutatja, mely kulcsok hiányoznak a többi
  nyelvből és melyek nem használtak; egy kulcsnál (`lang/en.json::board.title`) hol használják és mi az értéke
  nyelvenként; a forráskódnál pedig mely használt kulcsok nem léteznek (`⚠ missing in hu.json`). Lefedi a lang JSON-
  és PHP-fájlokat, valamint a `$t` / `t` / `trans` / `__` / `@lang` / `Lang::get` használatokat. A dinamikus kulcsokat
  (`` `x.${y}` ``, összefűzés) soha nem jelzi hiányzónak, a mondatszerű kulcsokat (a Laravel „strings as keys"
  megoldása) pedig soha nem jelzi nem használtnak. A TS / JS szótármodulok (`export default { … }` a `locales/en.ts`,
  `i18n/de.ts` fájlban) is nyelvfájlnak számítanak: rájuk is jelzi a hiányzó, nem használt és ismeretlen kulcsokat, a
  vázlatuk pedig a kulcsfát mutatja. A projekt saját `t()` burkolójával (egy helyi modul, amely a `vue-i18n`-re vagy
  az `i18next`-re épül és `t`-t exportál) a scriptekben lévő `t('a.b')` hívások is kulcshasználatnak számítanak. A
  kulcsok teljes alakban látszanak, a közös előtagot egyszer kiírva (`a.b.{c, d}`), és a nézet a fordítási fájl
  nélküli projektekben is gyors.
- **Stíluslap-osztályok használata a `--format classes` móddal:** megmutatja, mely template-ek és komponensek
  használnak egy osztályt, és kiad egy engedékeny listát a nem használtakról (a dinamikusan összeállított osztályokat
  és a Tailwind utility-ket soha nem jelzi). A lang fájlok, a stíluslapok és a `$t(…)` hívások a vázlatban rövid
  tippet is kapnak, az MCP `framework_view` eszköz pedig megkapta az `i18n` és `classes` értéket.
- **MCP `guide` eszköz `topic` paraméterrel:** az opcionális `topic` (pl. `{topic: "php"}`) az alap-útmutató
  helyett a skill egy részletező fájlját adja vissza, így a csak MCP-t használó kliensek is elolvashatják. Témák:
  `php`, `blade`, `js`, `jsx`, `vue`, `css`, `rust`, `swift`, `kotlin`, `python`, `go`, `gen`, `mcp`, `reference`.

### Változás

- **A JS/TS/Vue fájlkulcsok a projekt gyökeréhez relatívak:** a kulcs fájl-része (`ui/src/x.ts::fn`) a legközelebbi
  `.git`-et vagy `.callgraph`-ot tartalmazó mappához viszonyít (ha nincs ilyen, a csomag / workspace legfelső
  mappájához), minden platformon `/` elválasztóval. Monorepóban így egy fájlnak egy kulcsa van, a
  `--grep 'ui/src/x.ts::fn'` minden csomagból megtalálja a hívókat; korábban minden importáló csomag a saját relatív
  kulcsát látta. Az egycsomagos projektek kulcsai nem változnak. A tárolt gráf egyszer, csendben újraépül.
- **Rövidebb skill, nyelvenkénti részletek:** az agent skill mostantól egy jóval rövidebb, angol nyelvű `SKILL.md`
  mag, plusz a `ref/` mappa részletező fájljai (php, blade, js, jsx, vue, css, rust, swift, kotlin, python, go, gen,
  mcp, reference), amelyeket az agent csak a projekt nyelveihez olvas be. A Kotlin, a Blade és a React (`ref/jsx.md`:
  komponensek, hookok, jelenlegi korlátok) először kap leírást a skillben. A `self-update` az új szerkezetet telepíti.
- **Kevesebb zaj a framework-nézetekben:** a `--format events`, `middleware`, `validation`, `jobs`, `providers` és
  `di` csak a megfelelő helyen illeszti a mintáit, így pl. a `config` vagy a `response` már nem jelenik meg
  eseményként, az `X::dispatch` és az `Event::listen` viszont szerepel. A middleware címkéje már nem tapad a céljához,
  a validáció pedig csak a valódi szabálylistának számító stringeket veszi figyelembe. Ezekben a nézetekben a
  hivatkozás nem hívás, így a `belongsTo(Event::class)` nem jelenik meg esemény-dispatchként.
- **A Laravel-nézetek mutatják a drótozást:** a `--format events` `wired` sorokat (kiváltás helye → listener) és
  `observer` sorokat listáz; a `jobs` a `queued` osztályokat (`ShouldQueue`) és a jobok és notificationök `wired`
  sorait mutatja, az esemény-kiváltásokat pedig már nem; a `middleware` megmutatja, mely útvonalak használják az egyes
  middleware-eket; a `providers` csak a service provider osztályokon belüli regisztráló hívásokat számolja, így egy
  modell `boot`-ja vagy egy lekérdezés `bind`-ja (pl. tesztekben) nem jelenik meg.
- **Tisztább sorok a vázlatban és a nézetekben:** a PHP enum- és konstansolvasások `external_call` helyett
  `const_use` sorként látszanak; a `--format routes` sorai már nem végződnek `@ __top__::__top__`-ra, a Vue
  metódusindex sorain pedig nincs `[private]`.
- **Az egyfájlos kimenet relatív útvonalat használ:** ha a `show` kimenetében a projekt útvonala csak egyszer
  szerepel, a `FILE` útvonal relatív, és nincs `root:` sor, így bemásolható a következő hívásba.
- **A story-k tesztnek számítanak:** a story-fájlok (`*.story.*`, `*.stories.*`) és a `stories` / `storybook`
  mappák a projektprofilban tesztek, nem oldalak.
- **A Laravel űrlapellenőrzés követi a `form.transform(...)`-ot:** a `--format forms`, a `form:` sorok és a `laravel:`
  generátor űrlap ↔ szabály ellenőrzése mostantól azokat a kulcsokat vizsgálja, amelyeket a transform visszaad;
  `...data` szórás esetén az űrlapmezőket és a hozzáadott kulcsokat. A láncolt `form.transform(…).post(…)` hívással
  elküldött űrlapot is megtalálja, akkor is, ha a lánc több sorra van tördelve (`form` / `.transform(…)` /
  `.post(…)`).
- **A Laravel preset gondosabban választ űrlapkomponenst:** a komponens csak akkor kap szerepet, ha megvan rajta a
  kötött prop (a hibaállapot-blokkot már nem használja mezőhibaként). A mezőburkoló komponens (címke + hiba + slot,
  pl. `FormField`) körbeveszi a mezőket, a textarea és a checkbox komponenseket pedig a `text` / `bool` mezők kapják;
  a `.callgraph/gen.toml` kulcsai: `field`, `textarea`, `checkbox`. A preset a tsconfig `paths` / Vite alias mögötti
  űrlapkomponenseket is megtalálja (pl. `@ui` egy különálló UI-csomagra), a `.callgraph/gen.toml`-ban
  megnevezetteket is, és előnyben részesíti azokat, amelyeket a
  meglévő oldalak már importálnak.
- **Egyértelműbb jelölések a kimenetben (minden nyelven):** a hivatkozás-hívások `(ref)` jelölést kapnak a vázlatban,
  a `--grep` soraiban és a hívóknál. A teszt függvények `[test]`, a csak tesztből álló modul `[tests]` jelölést kap,
  és egy darabszám-sorrá van összecsukva. Az injektálási, navigációs és callback-bekötési élek a fajtájukat (`inject`,
  `nav`, `wire`) mutatják a kategória oszlopban.
- **A `gen` gyorsabb nagy PHP-projektekben:** az ismeretlen típusneveket egyetlen projektbejárással oldja fel
  (korábban nevenként egy bejárás), minden projektfájlt hívásonként legfeljebb egyszer olvas be, a Laravel preset
  pedig egyszer olvassa a `composer.json`-t.
- **Rövidebb MCP használati hibák:** a `generate`, `rename` vagy `undo` használati hibája MCP-n már nem fűzi hozzá a
  teljes használati szöveget; a CLI nem változik. A `generate` eszköz leírása is rövidebb — a nyelvtan az útmutatóban
  és a skill `ref/gen.md` fájljában van (MCP: `guide`, `topic: "gen"`).
- **A `laravel:` preset a `gen` generátorán át ír:** a FormRequest-ek, a controller-metódusok és az Inertia/Vue
  oldalak mostantól ugyanúgy formázódnak, mint maga a `gen`: PHPDoc `@return array<string, list<string>|string>`,
  `function submit()` a Vue oldalakon, a hosszú tagek attribútumonként tördelve, elöl az `@inertiajs/vue3` import, a
  `const props =` pedig csak ott, ahol a script használja. A Ziggyt a `composer.json` / `package.json` függőséglistái
  alapján ismeri fel, nem szövegkereséssel.
- **Stíluslap-hívások és JSON-kulcsok a vázlatban:** a CSS/SCSS hívások (`var()`, `@include`, függvények, `@extend`) a
  szabályuk alatt jelennek meg, és a `--grep` megtalálja őket; a `var(--brand)` hívás `var(--brand)` alakban
  szerepel, és `var`-ként és `--brand`-ként is megtalálható, a `translateY`, `blur`, `scale` és hasonló függvények
  pedig már nem rejtőznek el. A metódus nélküli JSON-fájl rövid kulcsindexet kap a
  metódusindexben (a felső kulcsok gyerekszámmal, a skalár sorozatok összevonva). A `--format summary` JSON-nál,
  Markdown/HTML-nél és stíluslapoknál „0 metódus" helyett kulcsokat, szakaszokat vagy szabályokat számol.
- **Pontosabb SCSS-vázlat:** a `&--#{$x}` és a `.span-#{$i}` megtartja a teljes szelektornevét; a többsoros
  szelektorokat és a mixin paraméterlistákat helyesen olvassa; a zárójel nélküli `@mixin` és a `@include x;` (hívás)
  is működik; a kommentekből és a szelektorfejekből már nem jelent hívást; az ugyanabban a scope-ban újraértékelt
  `$var` írás, nem második konstans; a sor végi `//` kommentek rögzítve vannak. A CSS és az SCSS egy közös motort
  használ.
- **Összevont Markdown-linkek:** a vázlat ugyanazt a link-célt egyszer listázza, `×N` jelöléssel.
- **Nincs ismétlődő tipp MCP-n:** a záró 💡 tipp sor már nem kerül minden MCP-válaszba (a CLI megtartja). A skill
  mostantól az md, json, css, scss és html fájlok szerkezetére is a callgraphot ajánlja.
- **Tömörebb Vue-vázlat:** a propok egyszer szerepelnek (a komponens propjait megismétlő `defineProps<X>` interfész
  már nem külön blokk), az emitek és a modellek egy-egy sorban állnak (`emits (N): a, b, …`, `models (N): name:
  Type`), a sornál hosszabb prop-alapérték `…`-tel le van vágva, a közös mappájú `component_use` útvonalak pedig
  egyszer írják ki a mappát (`dir/{atoms/A, organisms/B}.vue`). Egy nagy komponens vázlata így jóval rövidebb.
- **A nagy egyfájlos vázlat előbb tömörödik, csak utána jön a metódusindex:** a kimenet-korlát fölött a vázlat
  először kompakt alakban jelenik meg (megjegyzések nélkül, a konstansok és propertyk tördelve, a híváslisták 8 elem
  után `… +N`-nel rövidítve, minden tag megmarad); metódusindex csak akkor, ha ez sem fér bele. A `--full` a teljes
  vázlatot adja.
- **Csoportosított hívók az egysoros `--grep` nézetben:** ha a `--grep` kimenete a korlát fölött hívásonként egy
  sorra vált, minden fájl egyetlen fejsor, alatta a hívói (`Cont::caller  L12, L40 │ kód`); a minden sorban azonos
  cél egyszer, legfelül áll (`← cél`).
- **Rövidebb `--depth` kimenet:** a 2. szinttől a teszt- és story-hívók egy sorba kerülnek
  (`tesztek/story-k: N hívó M fájlban`), ha van más hívó is, a teljes kimenet pedig a kimenet-korlátnál le van vágva
  egy `⚠` sorral, amely megmondja, hogyan szűkíts. A `--full` mindent felsorol.
- **A többfájlos `--format summary` mutatja a mappát:** az azonos mappa sorai egy `mappa/` fejlécsor alatt
  csoportosulnak; az egyedülálló fájl a relatív útvonalával jelenik meg. A kimenet-korlát fölött a summary helyette
  mappánkénti összesítést ad (`dir/  N fájl · C konténer · M metódus`, a kis mappák felsorolják a fájljaikat), így egy
  nagy projekt is jóval rövidebb válaszba fér; a `--full` minden fájlt mutat.
- **Swift projektprofil:** az olyan mappák, mint a Features, Core, Shared, Networking, UI és Persistence, rétegeket
  kapnak; az `@Observable` / `ObservableObject` osztály store-jelölt, a `View`-t megvalósítók nézetek, a `Codable` /
  `Identifiable` típusok modellek. Több Swift- és Foundation-beépített van kategorizálva (`pow`, `defer`,
  matematikai függvények, `os_log`, UIKit- és Foundation-típusok).

### Javítás

- **A literális `import('…')` használatnak számít:** a fix stringgel írt dinamikus `import('./x')` vagy
  `import('pkg')` JS-ben, TS-ben és Vue-ban mostantól a fájlra vagy a csomagra mutat, így a `deps` már nem jelzi az
  ilyen csomagokat nem használtnak, az importáló komponens pedig felsorolja a betöltött fájlokat.
- **`deps`: a tesztkörnyezet-csomagok eszközök:** a `jsdom`, `happy-dom`, `jest-environment-*` és `@vitest/*`
  csomagokat már nem jelzi nem használtnak.
- **Vue: a `computed(…).value` metódusai nem Vue API-hívások:** `const x = computed(…)` vagy `ref(…)` esetén az
  `x.value.filter(…)`-hez hasonló hívás az adaton végzett egyszerű hívás, már nem Vue API-hívásként jelenik meg.
- **A `--grep` a láncolt hívás illeszkedő sorát mutatja:** a több sorra tördelt hívásláncnál a megjelenített
  forrássor az illeszkedés sora, nem a lánc első sora.
- **Az ismeretlen `--kapcsoló` hibát ad:** az elemzés elgépelt kapcsolója mostantól a használati szöveggel áll le,
  nyers JSONL-kimenet helyett.
- **A `laravel:` preset írás előtt szintaxist ellenőriz a generált `.ts`, `.js` és `.html` fájlokon is:** korábban
  csak a PHP- és Vue-fájlokat ellenőrizte.
- **Projekten kívüli JS/TS-fájlok modulútvonala:** a `package.json`, `tsconfig` vagy `jsconfig` nélküli fájl
  `home/…` alakú modulútvonalat kapott; most a fájl mappájához képest relatív.
- **Rust: a crate saját moduljai saját kódnak számítanak:** a `mod x;` és az inline `mod` útvonalak már nem számítanak
  csomagnak.
- **A `--grep <Komponens>.vue --depth N` minden szinten követi a template-használatot:** a 2. szinttől üres volt
  (Windowson is).
- **A controller-csoportok Laravel-útvonalai megjelennek a `--format routes` kimenetben:** a
  `Route::prefix(…)->name(…)->middleware(…)->controller(X::class)->group(…)` csoporton belüli útvonalak (arrow
  function-nel és beágyazott csoportokkal is) szerepelnek, a string action, pl. az `'index'`, pedig `X@index` lesz.
- **Ciklusban regisztrált Laravel-útvonalak:** az interpolált nevű útvonal (`->name("{$code}.x")`) miatt a
  `route('en.x')` „dinamikus, nem ellenőrizhető" lesz ismeretlen helyett, a `routes/` mappán kívüli, ilyen útvonalakat
  regisztráló segédfüggvényre pedig a program felhívja a figyelmet. Ugyanez igaz a számított prefixű vagy nevű
  csoportokra (`Route::prefix($code)->name($code.'.')->group(…)`, konfigból vezérelt ciklusok) a `--format ziggy`
  kimenetben.
- **Az egysoros, típusos `defineEmits` is olvasható:** a `defineEmits<{ login: []; save: [id: number] }>()` egy
  sorban is megadja az emit-listát (`--format api`).
- **Nincs téves hívás a Vue-attribútumok template literaljából:** a kötött attribútum template literaljában lévő
  szöveg (`` :title="`… név (pl. …)`" ``) már nem számít hívásnak (az ékezetes betűk váltották ki).
- **A `--format forms` nem hagy el csendben űrlapot:** a számított URL-re küldött űrlap (`form.post(path.value)`)
  szerepel a listán, az útvonala ismeretlen/dinamikus jelölést kap; ha a cél olyan URL vagy route-név, amely nincs a
  route-táblában (pl. egy csomag vagy az auth-scaffolding route-ja), az űrlap `? nincs a route-táblában` jelöléssel
  szerepel; a mégis kihagyott űrlapokat a nézet végén összesíti, az okkal, fájllal és sorral
  (`? 2 kihagyva: <ok> (a.vue L3, b.vue L9)`). A `useForm({…})` objektumban lévő template string vagy `#` már nem
  rontja el a mezők olvasását.
- **Az opcionális read-guard hook nem tekinti olvasásnak az írást:** a kimeneti átirányítás és a heredoc már nem
  olvasás, így a `cat >> f`, `cat > f <<EOF` vagy `2>f` nagy forrásfájlon nincs letiltva; a `cat f` és a `cat < f`
  továbbra is olvasásnak számít.
- **Az opcionális callgraph-first hook mappát is elfogad `--project`-ként:** a `--project <mappa útja>` (pl. egy git
  worktree) futás is számít az ott lévő fájlokra, nem csak a regisztrált projektnév, így a hook már nem írja, hogy
  arra a fájlra még nem futott callgraph.
- **A `--grep useX --depth N` kihagyja a destrukturált értékeket:** a `useX()`-ből destrukturált érték használatai
  már nem szerepelnek a `useX` hívásaiként.
- **A `callgraph.sh rename` működik:** a wrapper szkript már továbbítja (a súgó eddig is hirdette).
- **A `laravel:` preset követi a projektet és a migrációt:** TypeScript-projektben `<script setup lang="ts">`
  fejlécet és típusos propokat ír akkor is, ha még nincs `.vue` fájl, és olvassa a migráció oszloptípusát: a
  `text` oszlop nem kap `max:255`-öt, a `string('x', 100)` pedig `max:100`-at.
- **A `locate` hívószámai a nyelvcsaládon belül maradnak:** egy Vue-függvény már nem számolja az azonos nevű
  PHP-metódus hívóit.
- **Swift: az `Extensions` mappa saját kód:** a Swift-fájlokat tartalmazó `Extensions` mappát korábban vendorként
  kezelte.

### Eltávolítva

- **A magyar skill-változat és a `skill-sync`:** a skill és az MCP-útmutató csak angol; a CLI kimenetének nyelve nem
  változik (`callgraph set-lang hu`, `CALLGRAPH_LANG`). A `callgraph skill-sync` megszűnt, a `set-lang` már nem írja
  át a `SKILL.md`-t, a `self-update` pedig törli a régi `skill-i18n/` mappát.

## [1.0.2] — 2026-11-16

### Változás

- **A `gyökér:` sor mutatja a git ágat:** a többfájlos kimenetek (és a `locate`) tetején álló gyökérsor mostantól a
  projektgyökér ágával végződik, pl. `gyökér: /x/app (ág: feature/z)`; detached HEAD esetén `detached <sha7>` áll
  benne, git nélkül pedig nem változik. Az agent azonnal látja, ha nem abban a checkoutban néz körül, ahol az ágat
  váltották (másik klónra vagy worktree-re mutató `--project` alias, más mappában indított MCP-szerver).

### Javítás

- **A rövid `::method` kijelölő működik többkijelölős hívásban:** a `callgraph.sh src/Sample.php::alpha ::beta --body`
  mostantól mindkét törzset mutatja; a `::beta` az előző kijelölő fájlját használja újra. Korábban „Not found: <root>/"
  választ adott. A fájl nélküli, elöl álló `::method` érthető használati hibát ad, a súgó pedig felsorolja ezt az alakot.

## [1.0.1] — 2026-11-16

### Javítás

- **A csoportból betöltött Laravel route-fájlok megkapják a csoport előtagjait:** a `->group(base_path('routes/admin.php'))`,
  `->group(__DIR__.'/admin.php')`, a csoporton belüli `require`, illetve a `bootstrap/app.php` `then:` closure-jében lévő
  csoport által betöltött fájl mostantól örökli a csoport URI-előtagját és route-név előtagját. A meglévő láncolt
  neveket (pl. `admin.reports.show`) a `--format ziggy` már nem jelzi ismeretlennek (téves riasztás), a `rename route`
  pedig ugyanígy ellenőrzi a neveket.
- **A névvel ellátott Laravel csoport név nélküli route-jai megkapják a csoport nevét:** a saját `->name()` nélküli
  route a névvel ellátott csoportban mostantól a csoport összefűzött nevét kapja, ahogy a Laravel is (`Route::prefix('x')->name('a')->group(fn () =>
  Route::name('.b')->group(fn () => Route::get('/', ...)))` eredménye `a.b`), így a `route('a.b')` hívást a
  `--format ziggy` már nem jelzi ismeretlennek. Az a route-fájl, amelyet `require` tölt be, de kimaradt a szkennelésből
  (jellemzően gitignore-olt, csak helyi fájl, pl. `if (app()->isLocal()) require __DIR__.'/local.php';`), a lemezről
  beolvasásra kerül, és a route-jai beleszámítanak.

## [1.0.0] — 2026-11-16

### Új

- **Csomagfelismerés (composer, npm, yarn, pnpm, bun):** a callgraph beolvassa a manifesteket és a lock-fájlokat
  (offline), a `callgraph.sh profile` pedig egy `stack:` sorban mutatja a fontosabb csomagokat a lockolt verziójukkal
  (lock nélkül a deklarált tartománnyal, jelölve); az MCP indításkori profilja ennek tömör változatát kapja. A
  `profile --packages` minden közvetlen composer / npm függőséget felsorol (telepített és deklarált verzió, dev jelzés).
- **Látszik, melyik csomagot hívja a kód, és hol használ egy csomagot:** az outline a függőségbe menő hívásokat a
  csomag rövid neve alatt csoportosítja (`guzzle: GuzzleHttp\Client::request`), a JSON-kimenetben `package` mező van
  (a verziók fájlonként egyszer), a `locate` pedig „csomag” indoklást ad. A `callgraph.sh uses <csomag> [--calls]`
  (MCP `package_usage`) API-szimbólumonként megmutatja, hányszor használja a projekt, és melyik fájlban először; a
  `(type)` hívás nélküli importot jelöl, a `--calls` minden hívássort felsorol.
- **`callgraph.sh deps`** (MCP `project_profile` `deps: true`-val): a valószínűleg fölösleges, a hiányzó és a csak dev
  composer / npm függőségek, soronként egy, rövid indoklással. Téves jelzés a `.callgraphignore`-ban
  `deps-ignore <név>` sorral némítható.
- **Inertia-oldalak:** a PHP `Inertia::render('Board')` hívás az oldal fájljára mutat (`Pages/Board.vue`), az oldal
  outline-ja `rendered by:` sort kap, a `show . --format inertia` pedig oldalanként felsorolja a render-helyeket és a
  props-okat, jelölve a hiányzó oldalfájlt és azt a props-ot, amelyet az oldal `defineProps`-a nem deklarál, valamint
  a sehol nem renderelt oldalakat.
- **Laravel route-ok és Ziggy:** a callgraph a betöltött `routes/*.php` fájlokból felépíti a route-táblát (csoportok,
  resource-ok, nevek, `/api` előtag). Az outline-ban egy Vue / JS `route('events.show')` hívás mellett ott a metódus,
  az URI és a controller-akció (vagy `⚠ no route`), a controller-metódus pedig `route:` sort kap. A
  `show . --format ziggy` kilistázza az ismeretlen route-neveket és a sehonnan be nem töltött route-fájlokat; ha
  vannak dinamikus nevű route-ok, az ismeretlen neveket csak `?` jelöli.
- **Laravel űrlap-ellenőrzés:** egy Inertia `useForm({...})` űrlapot összevet annak a controller-akciónak a
  validációjával, ahová küld (FormRequest vagy inline `validate`): `form:` sor a küldő függvény és az akció alatt
  (küldött, de nem validált mezők, nem küldött kötelező mezők, vagy `N fields ✓`); a `show . --format forms` mindet
  felsorolja.
- **Laravel modell-összefoglaló:** egy Eloquent-modell alatt az outline `table:` (az oszlopok a migrációkból) és
  `fillable:` sort mutat, a relációs sorok mellett ott a cél, a `⚠` sorok pedig jelzik, ha egy fillable vagy cast nem
  oszlop. A `show . --format models` felsorolja a modelleket, a modell nélküli táblákat, és azt a táblát, amelyet két
  migráció is létrehoz törlés nélkül (egy friss migrate elbukna).
- **Gyors egyfájlos outline Laravel-projektben:** egy fájl outline-ja csak a szükséges route-, migrációs és
  űrlapfájlokat olvassa be, az eredményt pedig a `.callgraph/laravel.bin` fájlban tartja (csak ha a `.callgraph/`
  létezik; a fájlok módosítási ideje és mérete alapján ellenőrizve, magától újraépül, nyugodtan törölhető). A
  route-fájlokat útvonal szerinti sorrendben olvassa, így egy controller `route:` sorai minden nézetben ugyanabban a
  sorrendben jönnek.
- **Kódgenerátor, `callgraph.sh gen '<kifejezés>'`:** rövid, Emmet-szerű kifejezésből PHP-osztályt / -tagokat, Vue
  egyfájlos komponenst (script setup, template, style), composable-t, sima JS / TS modult és teljes HTML-oldalt ír, a
  projekt stílusában (névtér a `composer.json`-ból, importok a projektből, TS vagy JS, pontosvessző, behúzás). Vue
  scriptben a `use:{useX}@/composables/useX` nevesített import (puszta útvonalból magától az lesz, ha a fájlban csak
  `export function useX` van), a `type:Flag{flag:str,on?:bool}` interface (inline objektumtípus is megy, pl.
  `list<{a:str}>`), a `raw{…}` / `const:{a,b}=useX()` pedig szó szerint kiír egy utasítást.
  Célok: `--write <fájl>` (csak új fájl), `--into <fájl>[::Osztály] [--after <metódus>]`, `--replace <fájl>:L<a>-L<b>`
  vagy `::<metódus>`, ahol az `--expect <sha256-előtag>` elutasítja a cserét, ha közben megváltoztak azok a sorok; cél
  nélkül vagy `--dry`-jal csak kiírja a kódot. Fájlonként egy sorral válaszol. Írás előtt ellenőrzi az új szöveg
  szintaxisát (hibánál semmi nem íródik), egy meglévő fájl minden módosítása előtt mentést készít a
  `.callgraph/backups/` mappába (fájlonként a legutóbbi 10), a `callgraph.sh undo [<fájl>]` pedig visszaállítja a
  legfrissebb mentést.
- **Generátor: JS / TS modulok és HTML-oldalak:** a `mod:util>fn:join(p:list<str>):str{…}+const:MAX:int=10` modult
  ír, a `class:Svc<Base>[Iface]>…` osztályt (a tagok, mint PHP-ban: `+` public, `-` private, `#` protected). Alapból
  minden exportált, a `-fn:` privát függvényt ad, a `!fn:` / `!class:` `export default`-ot; `.ts` fájl típusokat kap,
  `.js` fájl JSDoc-ot. Az `--into x.ts[::Osztály]` tagokat szúr be, a `--replace x.ts::fn` vagy
  `x.ts::Osztály::metódus` egyet lecserél. A `html:Cím[lang=hu]>(head>…)+(body>…)` teljes dokumentumot ír (doctype,
  charset, viewport, title) a `meta:description=…`, `link:style.css`, `script:app.js` és `base:/app/`
  head-rövidítésekkel; bármilyen elem és attribútum használható, inline `style{…}`, `script{…}` és `comment{…}` is
  (a CSS-t és a JS-t írás előtt ellenőrzi). Az `--into x.html` a `<body>` végére vagy a `::#id` / `::.class` /
  `::tag` elembe szúr be, a `--replace x.html::#id` egy elemet cserél.
- **Laravel-funkció egy lépésben:**
  a `gen 'laravel:Fee[auth,prefix=admin]>create(name:str,amount:int,paid?:bool)+edit+index'` együtt megírja a
  FormRequesteket, a controller-metódusokat, a route-okat és az Inertia / Vue oldalakat, ellenőrzi, hogy az űrlap
  mezői egyeznek-e a szabályokkal, figyelmeztet az alapérték nélküli NOT NULL oszlopokra, amelyeket az űrlap nem
  küld, és egyetlen `undo` az összes fájlját visszavonja.
  Ziggy nélkül (nincs `tightenco/ziggy` / `ziggy-js`) az oldalak sima URL-eket (`'/notes'`) használnak `route('…')`
  helyett; a hosszú szöveg nevű `str` mező (body, content, description, …) javaslatot kap a `:text` használatára.
  A `--replace` nélküli `--expect` és az `--into` nélküli `--after` külön hibaüzenetet ad (az `--expect` a lecserélt
  sorok sha256-előtagja, nem a teljes fájlé), és a súgó felsorolja a mezőtípusokat.
- **`callgraph.sh rename route|class|method <régi> <új>`:** a teljes projektben átnevez — route-neveket (`->name()`,
  csoportok, `'profile.*'` minták, resource route-ok `->names()`-szel, és minden `route('x')` PHP-ban, Blade-ben,
  JS-ben és Vue-ban), PHP-osztályokat (deklaráció, fájl-áthelyezés, névtér, `use`, típusjelzések, docblock; a
  `--strings` a sztring-hivatkozásokat is átírja, a `--keep-table` megtartja az Eloquent táblanevet) és
  PHP-metódusokat (felülírások, feloldott hívások, callable tömbök, route-akciók). Minden módosított fájl szintaxisát
  előbb ellenőrzi, a `--dry` csak jelent, és egyetlen `undo` a teljes átnevezést visszavonja.
- **MCP `generate` és `rename` eszköz (opt-in):** csak akkor szerepelnek a listában, ha a szerver `--generate`-tel
  vagy `CALLGRAPH_GENERATE=1`-gyel fut; nélküle az eszközlista pontosan a korábbi.

### Változás

- **MCP-eszközjelzések:** a többi eszközzel ellentétben az opt-in `generate` és `rename` fájlokat ír, ezért
  `readOnlyHint: false` (és `idempotentHint: false`) jelzést ad; az összes többi eszköz továbbra is
  `readOnlyHint: true`.
- **Helyi statisztika és opt-in telemetria: új parancsfajták.** A `uses` / `package_usage`, a `deps`, a `generate`,
  az `undo` és a `rename` külön parancsként számít a helyi statisztikában és eseménynaplóban (ezen a gépen,
  `callgraph.sh stats`), és — csak ha bekapcsoltad a telemetriát — az elküldött napi számokban is, továbbra is név,
  útvonal és kód nélkül. A `callgraph.sh telemetry preview` megmutatja, mi menne el; kikapcsolás: `CALLGRAPH_STATS=0`
  (helyi), `callgraph.sh telemetry off` vagy `CALLGRAPH_TELEMETRY=0` (küldés).

### Javítás

- PHP: a teljesen minősített név (pl. `\LogicException`, `\Helper`) mostantól a globális névtérből oldódik fel, nem az
  aktuális névtérből vagy egy importból.
- Az a SCSS-fájl, amelyben egy függvényhívás előtt ékezetes szöveg áll (`// Fejléc háttér`), már nem dönti össze a
  projekt-szintű nézeteket, például a `locate`-et, a `profile`-t vagy a framework-nézeteket (a 0.9.5-ben is így volt).
  Ha egy nyelvi elemző összeomlik egy fájlon, csak az a fájl marad ki, nem az egész parancs bukik el.

## [0.9.5] — 2026-11-13

### Új

- **Helyi parancsnapló**: a `callgraph.sh stats --events [--days N]` kilistázza a legutóbbi hívásaidat (parancs, kliens,
  eredmény, idő- és méretsáv, hogyan viszonyul az előző híváshoz, és benne volt-e a fájlja / szimbóluma az előző hívás
  kimenetében) és a leggyakoribb parancs-átmeneteket. Az „az előző kimenet felhasználása” összesítő átmenetenként
  mutatja, hányszor volt a következő hívás fájlja az első felsorolt fájl (`hit1`), a 2.–5. (`hit5`), egy későbbi
  (`hit`), vagy nem volt benne (`miss`), és benne volt-e a szimbóluma (`hit` / `miss`). A napló ezen a gépen marad
  (14 nap), és nincs benne név, útvonal vagy kód; az ellenőrzéshez csak az utolsó hívás kimenetének elemei maradnak meg,
  kulcsolt hash-ként a helyi `session.json`-ban, minden hívásnál felülírva, és soha nem mennek el. Kikapcsolás:
  `CALLGRAPH_STATS=0`.

### Változás

- **Opt-in telemetria (schema 2)**: ha bekapcsoltad a telemetriát, a napi számokban mostantól a parancsok sorrendjének
  számai (melyik után melyik jött, mennyi idő telt el köztük, ugyanarra a fájlra, mappára vagy projektre irányult-e),
  a munkamenet-hossz sávjai, a célfájl nyelve és méretsávja, valamint parancs-átmenetenként az előző kimenet
  felhasználásának sávjai (`hit1` / `hit5` / `hit` / `miss`, szimbólumnál `hit` / `miss`) is benne vannak — név,
  útvonal, kód és hash továbbra sem. Egyszer szólunk róla; a `callgraph.sh telemetry preview` pontosan megmutatja, mi
  menne el.
- Az indításkori ellenőrzés legfeljebb 10-szer (2 naponta) javasolja a használati számok megosztását, nemleges válasz
  után soha.
- **Az MCP-eszközök jelzik, hogy csak olvasnak, és hogy melyiket mikor érdemes használni:** mostantól minden
  MCP-eszköz kifejezetten megadja a `readOnlyHint: true`, `destructiveHint: false`, `idempotentHint: true` és
  `openWorldHint: false` jelzést, mert egyik sem módosít semmit, és nem nyúl ki a projektből, így az ezeket a
  jelzéseket megkövetelő katalógusok (pl. az OpenAI-é) is elfogadják a szervert. A `locate`, az `outline`, a `search`
  és a `framework_view` leírása most megmondja az agentnek, melyik való az adott feladatra: az `outline` `method`-dal
  felsorolja, mit hív az a metódus (ezért nincs külön callees eszköz), a fordított irányhoz (ki hívja) pedig a
  `find_callers`-t ajánlja.

### Javítás

- A `callgraph.sh telemetry status` mostantól a `DO_NOT_TRACK`-et is felsorolja a beküldést tiltó okok között.

## [0.9.4] — 2026-11-12

### Új

- **Ellenőrzés a session elején** (opcionális `SessionStart` hook, `callgraph.sh hook install-session`; a
  telepítő rákérdez): új session elején szól, ha egy ajánlott beállítás ki van kapcsolva (frissítő hook, MCP
  `--auto-update`-tel, a használati számok megosztása) vagy újabb verzió van, és az agent megkérdez, mielőtt
  bármit módosítana. Tételenként legfeljebb 2 naponta és 3-szor; `hook never <tétel>`, `CALLGRAPH_RECOMMEND=0`.

- **MCP Registry**: minden kiadás mellé `callgraph.mcpb` is készül (MCPB-csomag mind az öt platform
  binárisával), és felkerül a hivatalos MCP Registry-be `io.github.GeRiY/callgraph` néven, így az MCPB-t ismerő
  kliensek (pl. Claude Desktop) egy lépésben, Rust nélkül telepíthetik az MCP szervert.

### Változás

- A frissítés-ellenőrzés és a letöltés az `update.callgraph.eu`-n át megy (tartalék: GitHub); névtelen napi
  használat-számlálás — lásd a README „Névtelen napi számlálás” részét.
- `telemetry reset` (újra döntetlen, kikapcsolva) és `hook reset [tétel]` (a `hook never` visszavonása); a
  `DO_NOT_TRACK=1` mostantól a telemetriát is leállítja.

### Javítás

- A kiadási zipekben a bináris és a szkriptek megtartják a futtatási jogot, így a kézzel kicsomagolt csomag
  is `chmod +x` nélkül indul (a telepítő eddig is beállította).

## [0.9.3] — 2026-11-12

### Új
- **HTML-fájlok (`.html`, `.htm`):** dokumentumként, a Markdownhoz hasonlóan — az outline a `<title>`-t és a
  `<h1>`–`<h6>` címsorokat mutatja szintenként, sortartománnyal, a `--lines` működik, a `--strings` pedig a látható
  szövegben és a `title` / `alt` / `placeholder` / `aria-label` / `value` attribútumokban keres (a `<script>`, a
  `<style>` és a komment kimarad). A hibás vagy nem zárt HTML nem akasztja meg az elemzést.
- **A `locate` megtalálja a property-ket és mezőket:** a PHP property (a `protected $fillable` és a promoted
  constructor property is), a TS / JS class field és parameter property, a Rust struct-mező és a Swift / Python / Go
  property definíció (`property $fillable  L12  ← "fillable" (exact property)`; a `fillable`, a `$fillable` és az
  `Invoice::$fillable` is illik rá); a név része név-részlet. A sok fájlban definiált property (pl. a `$fillable`
  minden modellben) nem szorítja hátrébb a pontos osztályt és a ritkább kifejezés fájljait.

### Változás
- **A `locate` elöl hozza a több kifejezést egyesítő neveket:** ha egy definíció vagy fájlnév a keresett kifejezések
  közül kettőt vagy többet tartalmaz (`toggleFavorite` a `toggle favorite`-ra, `PodcastEpisodeController` a
  `podcast episode`-ra), nagy, a kifejezések számával növő pontot kap, akkor is, ha a kifejezések külön-külön túl
  általánosak (a „túl általános” sor megmarad). A típusdeklarációs fájlok (`.d.ts`, `.pyi`) definíciói kevesebbet
  érnek a valódiaknál, és már nem tolják a lista elejére ezeket a fájlokat.

### Javítás
- **PHP: a Laravel-konténerből kért fogadó feloldása:** a `$x = app(Foo::class)`, `resolve(Foo::class)`,
  `app()->make(Foo::class)`, `$this->app->make(Foo::class)` és `App::make(Foo::class)` után `$x` típusa `Foo` (a `use`
  és a névtér szerint feloldva, mint a `new Foo()`-nál), az `app(Foo::class)->bar()` / `resolve(Foo::class)->bar()`
  pedig `Foo::bar`-ra oldódik, így a `--grep 'Foo::bar'` `--guess` nélkül is megtalálja ezeket a hívókat. A
  sztring-azonosító (`app('foo')`) feloldatlan marad.

## [0.9.2] — 2026-11-12

### Javítás
- **JS / TS literál kulcsú computed hívás célpontja:** az `o['x']()`, `o["x"]()` és ``o[`x`]()`` ugyanazt a célpontot
  kapja, mint az `o.x()` (a régi `[]` cél megszűnt); az `o[k]()` `dynamic_call` marad, a szám kulcs (`a[0]()`) már nem ad `[]`-t.
- **`--guess` egyszintű `--grep`-nél:** a `--grep 'Típus::metódus' --guess` (és a `--depth 1 --guess`) a feloldatlan,
  csak név szerinti hívókat is listázza, `?` jellel, mint a `--depth 2+` nézet.
- **Láblécek az egyszintű `--grep 'Típus::metódus'`-nál:** a `--depth 2+` nézet két sora (`(+N feloldatlan, csak név
  szerinti egyezés kihagyva — --guess mutatja)` és `(N hívó dinamikus hívást tartalmaz — kézzel nézd át)`) az egyszintű
  nézetben is megjelenik, ha van mit mondani.

## [0.9.1] — 2026-11-11

### Új
- **A JSX komponens-használat hívás:** a nagybetűs JSX-elem (`<Button />`, `<Button>…</Button>`, `<Ui.Button />`)
  `.tsx` / `.jsx` / `.js` fájlban `component_use` hívás (ugyanaz a kategória, mint a Vue-ban), az importból úgy oldódik
  fel, mint a közönséges függvényhívás, így a `--grep 'Button$'` és a `--depth` megtalálja a komponens használóit; a
  fájlon belül definiált komponens `internal_helper`. A `<div>` és a többi kisbetűs elem, valamint a fragment nem hívás.
- **Vue `--format api`:** kiírja a komponens `props:` (név, ismert típus, `[required]`) és `emits:` listáját.

### Javítás
- **JSX a sima `.js` / `.mjs` / `.cjs` fájlban** már nem ad `syntax error: Unexpected JSX expression` hibát és üres
  outline-t: ezeket a fájlokat JSX-képesként parse-oljuk (mint a CRA / babel / vite projektek). A `.ts` nem JSX
  (ott a `<T>x` típus-állítás).
- **Névterű `--grep` minta** (`App\Svc\Real::m`): az azonosító-karakter után álló, nagybetű előtti egyetlen `\`
  névtér-elválasztó, nem regex-escape (a `\S`, `\R`, `\B` eddig csendben rosszul egyezett vagy hibát adott); a `\.`,
  `\d`, `\w`, `\b` és a kétszerezett `\\` változatlan. A még mindig hibás minta mellé 💡 sor kerül a kétszerezésről.
  Ugyanez a `--depth`-re és a `callers`-re is.
- **`--layer` / `--layers` / `--by-layer` útvonal nélkül** a projekt gyökerén fut (mint a `profile`), nem a használati
  sort írja ki.
- **A `protect` 🔒 jelölés** a `--format summary`-ban (és a `stats` fejlécében) is látszik; a json, mermaid és dot
  változatlan.

## [0.9.0] — 2026-11-11

### Új
- **`dynamic_call` jelölés:** a futásidőben eldőlő célú hívások eddig hiányoztak az outline-ból, így a gráf ott is
  teljesnek látszott, ahol nem az. Mostantól szerepelnek (a többi kategória után, a lista végén), célnak a hívott
  kifejezéssel — PHP: `$obj->$m()`, `$obj->{$expr}()`, `$class::$m()`, `$fn()`, `new $class`, `call_user_func($x)`,
  `app($var)` / `resolve($var)` / `$this->app->make($var)`; JS / TS / Vue: `obj[expr]()`, `new (expr)()`,
  `import(expr)`, `require(expr)`; Python: `getattr(obj, név)()`, `globals()[név]()`, `importlib.import_module(név)`,
  `eval` / `exec`. A literál esetek (`obj['x']()`, `app(Foo::class)`, `getattr(obj, 'x')()`) közönségesek maradnak.
  Jelölés, nem hívó-él: a `--grep`, a `--depth` és a `locate` figyelmen kívül hagyja; a `--depth` egy sorban jelzi,
  hány hívó tartalmaz dinamikus hívást („kézzel nézd át”). PHP-ban a `Closure` / `callable` típusú callback-paraméterek és a helyi closure-ök (`$f = fn …`) nem számítanak dinamikus hívásnak, az osztály-típusú property hívása (`($this->invokable)()`) pedig sima `Típus::__invoke` hívás. Szűrés: `--category dynamic_call`. Rust, Swift, Go: még nincs.
- **Helyreállító tippek:** az üres vagy hibás eredmény mostantól megmondja a következő lépést, nem marad néma. A találat
  nélküli `--grep` elmagyarázza, hogy a minta reguláris kifejezés a hívás céljára, javasolja a név szerinti mintát
  (`(^|::|\.|->)név$`, `--guess`) és az `esetleg erre gondoltál: User::find` javaslatot; a `Nem található: <út>` és a
  `Nincs ilyen metódus` egy második `Esetleg erre gondoltál: …` sort kap (legfeljebb 3 hasonló útvonal / metódusnév);
  a `--depth` megmondja, hogy a cél nem létezik, vagy létezik, de senki sem hívja; a találat nélküli `--strings`
  elmagyarázza az illesztési szabályt; a találat nélküli `locate` névrészeket, angol kódneveket és `--strings`-et
  javasol. Az ismeretlen `--category` mostantól hiba az ismert kategóriák listájával
  (`Ismeretlen kategória: … (ismertek: …)`), nem üres eredmény. A tippek csak a szöveges kimenetben jelennek meg; a
  json, mermaid, dot, stats és a `next` javaslatok változatlanok.
- **Receptek a skillben:** rövid „Receptek — feladat → hívássorozat” szakasz (hibajavítás, ki hívja ezt, ismeretlen
  projekt, hol van ez a szöveg) a skill-utasításban, és ugyanez 3 sorban az MCP-utasításban.

## [0.8.0] — 2026-11-11

### Új
- **`--help` a binárisban:** a `callgraph --help`, `-h` és `help` a teljes, naprakész súgót írja ki (angolul vagy
  magyarul) — ugyanazt a szöveget, mint a `callgraph.sh --help`; most már a Python, a Go, a `--signatures`, a
  `protect` és a környezeti változók is benne vannak.
- **Python (1. fázis):** `.py` / `.pyi` fájlok a térképben (tree-sitter): osztályok ősökkel és dekorátorokkal,
  függvények, metódusok (`async`, `@staticmethod`, `@classmethod`, `@property`), konstansok, `__all__`, docstringek,
  szignatúrák (`--signatures`, `--format api`), importok, kategorizált hívások (`native_python`, `internal_helper`,
  `static_call`, `external_call`), stringek, szintaktikai hibák, Django / Flask / FastAPI felismerés. A `venv`,
  `.venv`, `site-packages` és `__pycache__` mappák alapból kimaradnak.
- **Go (1. fázis):** `.go` fájlok a térképben (tree-sitter): structok (mezők property-ként, beágyazott típusok),
  interfészek, típus-aliasok és definiált típusok, függvények és receiveres metódusok (akkor is, ha a típus másik
  fájlban van), generikusok, `const` / `var`, importok (alias, blank, dot), több visszatérési értékű szignatúrák
  (`--signatures`, `--format api`), kategorizált hívások (`native_go`, `internal_helper`, `static_call`,
  `instantiation`, `external_call`; a `go` / `defer` is), stringek, doc-kommentek, nagybetűs név = public,
  szintaktikai hibák, Gin / Echo / Fiber / Chi / gRPC / GORM felismerés. A `testdata` alapból kimarad (a `vendor`
  eddig is); a `_test.go` fájlok a `locate`-ben tesztnek számítanak.

## [0.7.0] — 2026-11-11

### Új
- **`protect` sorok a `.callgraphignore`-ban:** a `protect migrations/` a találó fájlokat a térképben tartja (teljesen
  lekérdezhetők), de az outline-ban, `--body`-ban, `--lines`-ban, `locate`-ban és `--format json`-ban
  (`"protected": true`) `🔒`-kal jelöli: védettek, olvasd, de ne szerkeszd őket. Ugyanazok a mintaszabályok, mint a
  kizárásnál; `protect` sor nélkül nincs változás.
- **Szignatúrák az outline-ban:** a `--signatures` (vagy `CALLGRAPH_SIGNATURES=1`) a normál outline és a `--grep`
  hívó-outline metódussorában a tömör szignatúrát írja ki: `updateOrders(array $ids, bool $notify = false): array  L4-9`
  (ahol a nyelv adja a paramétereket / visszatérési típust). A `--format api` mostantól a TS / Vue visszatérési
  típust, PHP-nál pedig a pontosabb DocBlock-típust is mutatja (`@return array{updated:int}`, `list<int>`, `@param`
  shape), legfeljebb 80 karakterben.

### Változás
- **A nem UTF-8 forrás dekódolva jelenik meg, nem `�`-ként:** az ISO-8859-2, windows-1250 és ISO-8859-1/windows-1252
  fájlokat felismeri és helyesen olvassa (megjelenítés, outline, `locate`, `--strings`); a szemét továbbra is `�`.
- **Erősebb változásészlelés (Linux / macOS):** a frissesség-bélyeg az inode-változás idejét (ctime) is tartalmazza,
  így az azonos méretű, visszaállított mtime-ú szerkesztés is észrevehető. A tárolt gráf egyszer, csendben újraépül.

## [0.6.1] — 2026-11-11

### Javítás
- **A nem UTF-8 (pl. ISO-8859-2 / Latin-2) fájlok már nem adnak üres eredményt:** a `--lines` és a `--body` /
  az MCP `method_body` egy más kódolású, ékezetes szöveget tartalmazó fájlnál (akár egyetlen kommentben) semmit
  nem írt ki, üzenet nélkül. Most kiírja a sorokat, az érvénytelen UTF-8 bájtokat `�`-ként mutatva; a sorszámok
  helyesek maradnak.

## [0.6.0] — 2026-11-10

### Új
- **Javasolt következő lépés (opt-in):** `CALLGRAPH_NEXT=1` vagy `--next` mellett (MCP: `callgraph mcp --next`) egy
  eredmény egy ilyen sorral végződhet: `↪ r7 lehetséges következő: [1] OrderService.php::refund --body  [2] refund
  hívói  [3] outline OrderService.php`. Az agent a számával futtatja (`callgraph.sh next r7 1`, több: `next r7 1,3`;
  MCP: a `next` eszköz), nem neki kell összeraknia a parancsot. Csak olvasó navigáció, csak a kimenetben szereplő
  nevekből, legfeljebb 3, és ha semmi sem egyértelmű, nincs sor (hiba, üres eredmény, json / mermaid / dot). A projekt
  `.callgraph/results/` mappájában tárolódik (az utolsó 50, 24 óráig); a `callgraph.sh stats` mutatja, milyen gyakran
  használták.
- **A használati statisztika opt-in beküldése:** `callgraph.sh telemetry status|preview|on|off`. Semmi nem megy el,
  amíg a felhasználó terminálban be nem kapcsolja (a `telemetry on` y/N-t kérdez; agent nem teheti meg). Utána a
  lezárt napok sávos számai (ugyanazok, mint a `stats`-ban, út és név nélkül) naponta egyszer, a háttérben mennek el;
  a `CALLGRAPH_TELEMETRY=0` végleg letiltja. A telepítő egyszer rákérdez (Igen / Most nem / Soha), csak interaktív
  terminálban.

- **Jegyzet-jelölő a találatokon:** a sor végi `📝 N` mutatja, hogy a találathoz jegyzet tartozik — a `--grep`
  hívóinál (outline, hívásonként egy sor, minden `--depth` szint; MCP `find_callers`), a `locate` fájl-sorain és az
  `--format api` metódus-sorain. A metódus-soron a metódus jegyzetei számítanak; ha neki nincs, a fájléi. Az árva
  jegyzetek nem számítanak; jegyzet nélkül a kimenet változatlan.

### Változás
- **A `stats` külön számolja a felhasználói és a belső hibákat:** a hibás használat, a nem létező és a nem támogatott
  fájl „felh. hiba” (0.5.1 óta nem kerül az errors.jsonl-be); a „belső” hibák az errors.jsonl-ben is ott vannak. A
  `callgraph.sh`-ban a nem támogatott / generált / bináris fájl mostantól felhasználói hiba (error-id nélkül).
- **Egyszeri tájékoztatás:** amikor egy gépen először rögzül helyi statisztika, egy sor a stderr-en jelzi (és hogy
  hogyan kapcsolható ki). MCP-kimenetben soha.

## [0.5.3] — 2026-11-09

### Új
- **`--strict` CI-hez:** a szintaktikai hibás fájl alapból továbbra is 0-s kilépési kódot ad (a részleges térkép is
  hasznos). A `--strict` kapcsolóval (vagy `CALLGRAPH_STRICT=1`-gyel) a `show` (fájl, mappa, `--grep`), a `locate` és a
  `profile` 3-as kóddal lép ki, ha egy beolvasott fájl szintaktikai hibás; a kimenet ugyanaz.
- **`mail` és `notification` réteg:** a Laravel levél-osztályai (`extends Mailable`, jellemzően az `app/Mail`-ben) és
  értesítései (`extends Notification`) saját réteget kapnak az „ismeretlen” helyett. Az `event`-hez hasonlóan ehhez
  bizonyíték kell a kódban: egy csak `Mail` nevű mappa nem elég.

### Változás
- **Az `irány:` sor csak valódi hívás-párokat mutat:** a láncban minden `A → B` azt jelenti, hogy A ténylegesen hívja
  B-t. Az a réteg, amely csak a lánc közepébe hív, mellékágként látszik, pl.
  `irány: controller → service → model; command → service` (korábban: `controller → command → service → model`,
  pedig a controllerek nem hívnak commandot).
- **Az enum nem kap szerepet a nevéből:** az `app/Enums/PaymentProvider.php` csak a neve miatt volt „provider”. Egy
  enum (PHP, TS, Rust, Swift) mostantól nem kap szerepet a nevéből; az `Enums` mappa `enum`-ként látszik.
- **A bemásolt framework-mag a vendor-kódhoz hasonlóan kimarad:** egy régi projektbe bemásolt framework (CodeIgniter
  `system/`, Yii `framework/`, Kohana `system/`) már nem ad `command` / `handler` / `request` réteget a profilhoz.
  Csak a framework-mag marad ki, a mellette lévő alkalmazás-kód továbbra is számít. Egy teljesen kihagyandó nagy
  almappa útvonalként a `.callgraphignore`-ba írható (`sn_monitoring/`).

### Javítás
- **A szintaktikai hiba már nem rejti el az utána jövő kódot (JS / TS / Vue, Rust):** korábban csak a hiba előtti
  függvények kerültek a térképre; egy Vue `<script setup>`-ban az első függvény hibája után semmi sem maradt. Most a
  következő felső szintű függvénytől, osztálytól, elemtől folytatódik, helyes sorszámokkal; csak a hibás rész hiányzik.
- **A `--layer` / `--layers` / `--by-layer` egyezik a profillal:** csak a mappanevekből soroltak be, ezért az
  `app/Actions/Event/*Action.php` `[event]`-ként látszott, holott a profil nem mutatott event-réteget. Most ugyanazokkal
  a szabályokkal és kódbeli bizonyítékkal dolgoznak, mint a `profile` (a projekt-térképből; nélküle egy `Event`-szerű
  mappa a következő szerepet adja, pl. `service`). A vendor- vagy build-kimenetként kihagyott fájlok itt sem kapnak
  réteget.
- **`callgraph.sh --project <mappa>`:** egy mappa-út (abszolút vagy relatív) regisztráció nélkül is működik minden
  paranccsal; korábban a fájl-lekérdezés azt írta: „Nincs projects.json — regisztrálj előbb”. Ismeretlen aliasnál
  továbbra is hibaüzenet jön.

## [0.5.2] — 2026-11-09

### Új
- **`update --dev`:** a `develop` ág legfrissebb buildjét telepíti (verzió: `X.Y.Z-dev.<commit>`, a release-ekhez
  hasonlóan a `SHA256SUMS` alapján ellenőrizve); az `update --check --dev` csak megmondja, van-e újabb. Egy sima
  `update` visszavisz a legfrissebb stabil release-re. A napi háttér-ellenőrzés és az MCP `--auto-update` maradnak a
  stabil release-eknél. Dev buildön a `callgraph --version` a dev verziót mutatja.

### Változás
- **A változásnapló magyarul is:** a `CHANGELOG.hu.md` minden release-t magyarul tartalmaz, és a magyar weboldal ezt
  mutatja.

### Javítás
- **`update --check` közvetlenül egy release után:** egy új release után még néhány percig azt mondhatta, hogy
  „naprakész”, mert a GitHub rövid ideig a régi választ adta. Most cache nélkül kérdez, és a legutóbbi release-ek
  listáját is megnézi, így az új verziót azonnal megtalálja.

## [0.5.1] — 2026-11-09

### Új
- **Helyi használati statisztika:** a `callgraph.sh stats` megmutatja, hogyan használtad (te és az agentjeid) a
  callgraph-ot az elmúlt 7 napban (`--days N`, legfeljebb 90): parancsonként és MCP tool-onként hány hívás volt, hány
  hibázott, mennyi ideig tartottak és mekkora volt a kimenet, a `locate`-nél pedig azt, hogy az első találat milyen
  gyakran volt az a fájl, amit az agent utána megnyitott. Minden a gépeden marad: semmit nem küld el, és nem rögzít
  útvonalat, nevet vagy keresett szöveget — csak sávos számokat. Az adat a skill mappájában van (`stats/`, kb. 90
  nap); kikapcsolás: `CALLGRAPH_STATS=0`.
- **`--no-gitignore`:** bármelyik parancshoz (show, mappanézetek, `locate`, `profile`, `refresh`, `callers`) adva a
  `.gitignore` által kizárt fájlokat is feltérképezi; az MCP serverhez és a hookhoz: `CALLGRAPH_GITIGNORE=0`. A
  `.callgraphignore` és a mindig kihagyott mappák (`vendor`, `node_modules`, …) ilyenkor is érvényesek.
- **`locate route::events.show`:** route-ot keres a neve alapján: elöl a definíció (`->name('events.show')` a
  `routes/*.php`-ban), utána minden `route('events.show')`, `to_route(…)` és `redirect()->route(…)` a PHP- és a
  JavaScript / TypeScript / Vue kódban. Ha nincs találat, kiírja, mit keresett.
- **`callgraph.sh --version`** (és `-V`, `version`): kiírja a verziót, a platformot és a binary helyét.

### Változás
- **A `.callgraphignore` útvonalat is elfogad:** az `md`, `.md` vagy `*.md` sor továbbra is egy kiterjesztést hagy
  ki, minden más sor viszont útvonal-minta, ugyanúgy írva, mint a `.gitignore`-ban (`public/build/`, `/storage/`,
  `!keep.js`), a fájl mappájához képest. Egy puszta szó, például `build`, továbbra is kiterjesztés; mappához
  `build/` kell.
- **A `locate` elöl a pontos egyezéseket rangsorolja:** az a fájl, amelynek osztály-, komponens- vagy függvényneve
  (vagy fájlneve) az egyik keresett kifejezés, mindig megelőzi azokat, amelyek csak szövegként tartalmazzák a
  kifejezéseket, például a fordítási fájlokat vagy a build-kimenetet.
- **Az idézőjeles kifejezés egyetlen `locate`-kifejezés:** a `locate "Szervező további eseményei"` a teljes szöveget
  keresi a kód stringjeiben (feliratok, fordítások), ahelyett hogy szavakra bontaná, amelyek oda nem tartozó nevekre
  illettek.
- **A `locate` mindig ír `root:` sort**, és ahhoz relatív útvonalakat, akkor is, ha csak egy fájl illik.
- **A profile elöl a fő layereket mutatja:** az architektúra layerei (controller, service, model …) jönnek elöl,
  fájlszám szerint, a tesztek, a migrációk és a konfiguráció a végén; így egy fő layer, például a `model`, már nem
  bújik a `… +6 more layers` mögé, miközben az egy-két fájlos layerek látszanak.
- **A `--by-layer` minden layerhez fejlécet ír:** a hívók minden csoportja egy ilyen sorral kezdődik:
  `[service]  3 calls`, a normál és a rövid („hívásonként egy sor”) `--grep` nézetben, és a `--depth` minden
  szintjén. A hívott metódushoz legközelebbi csoport van elöl (model → repository → service → controller), ahogy a
  súgó írja; korábban csak egyes sorok végén egy `[service]` címke jelezte a layert.
- **A felhasználói hibák már nem tool-hibaként kerülnek a naplóba:** egy elírt layernév, egy ismeretlen kapcsoló
  vagy egy nem létező fájl most csak az üzenetet írja ki — nincs `[error-id: …]` sor, nincs hibajelentő link, és az
  `errors.jsonl`-be sem kerül semmi. A valódi belső hibák továbbra is naplózódnak, és error-id-t kapnak.
- **A JSON outline az értékeket is mutatja:** `scripts.build = "vite build"` és `devDependencies.vite = "^6.0"`
  a puszta `string` helyett; a számok, a `true` / `false` és a `null` ahogy vannak, a 60 karakternél hosszabb
  stringek `…`-lel levágva, az objektumok és listák a méretükkel (`object (3)`, `array (2)`).
- **Egy JSON-kulcs kiolvasása:** a `callgraph.sh package.json::scripts --body` a kulcsot a pontos soraival írja ki;
  mélyebb kulcshoz pont (`scripts.build`), listaelemhez szám (`files.0`). Korábban „No such method” volt a válasz.
- **A `locate` megtalálja a JSON-kulcsokat:** a `locate laravel-vite-plugin` most kilistázza a `package.json`-t a
  kulcs sorával. Csak a pontos kulcsnév (vagy a teljes, pontokkal tagolt útvonal) számít, így a más szavakra
  keresést nem árasztják el a JSON-fájlok.
- **A Markdown-címsorok a szintjüket mutatják:** az outline behúzza az alcímsorokat, `#`, `##`, `###` jelöléssel, és
  már nem ír `()`-t a címsor után (`## Docker / build  L51-61`).
- **A CSS- és SCSS-szabályok már nem függvényként jelennek meg:** `.p-toast  L29-31` a `.p-toast()` helyett; a
  mixinek és a függvények megtartják a `()`-t.

### Javítás
- **A CSS outline minden szabályt listáz:** egyes szabályok hiányoztak (a `.p-toast` selectorú szabály mindig
  kimaradt, és a `@charset` utáni első szabály is), az at-rule-ok pedig egyáltalán nem jelentek meg. Most minden
  szabály és at-rule a soraival szerepel: `@import`, `@tailwind`, `@media`, `@supports`, `@layer`, `@keyframes`,
  `@font-face`, valamint a Tailwind v4 `@theme`, `@utility`, `@custom-variant`, `@variant`, `@plugin`, `@source`
  és `@config`.
- **A CSS-szabályok a helyes sorban kezdődnek:** egy komment vagy egy másik szabály utáni szabály azt a sort
  mutatta, ahol az előző szabály véget ért (`.p-datepicker-panel  L31-41` az `L39-41` helyett).
- **Az idézőjelek közötti `/*` nem vágja el a CSS-útvonalat:** a `@source '../**/*.blade.php'` elvágódott, mert az
  útvonalban lévő `/*`-ot komment kezdetének vette.
- **A profile már nem keveri a PHP- és a JavaScript-layereket:** egy PHP-controllereket és AngularJS (vagy más
  JavaScript) controllereket is tartalmazó projektben a profile egyetlen `controller` layert mutatott mindegyikkel.
  Egy több nyelvben is előforduló szerep most külön jelenik meg, pl. `controller (php)` és `controller (js)`, és a
  hívásirány (`flow:`) már nem köti össze a backendet és a frontendet egy közös néven, például a `service`-en át. Az
  egynyelvű projektek pontosan úgy néznek ki, mint korábban. A `--layer controller` továbbra is mindkettőt
  megtalálja; a `--layer controller:js` (vagy `--layer "controller (js)"`) csak a JavaScripteseket.
- **A külső kód már nem adja a projekt architektúráját:** egy projektbe másolt SDK vagy library (saját
  `composer.json`, `package.json` vagy `LICENSE` fájllal rendelkező almappa, vagy olyan mappák, mint `vendors/`,
  `third_party/`, `extensions/`, `external/`, `libs/`) olyan layereket és mintákat (Builder, Factory, Handler…)
  adott a profile-nak, amelyek a projektben valójában nincsenek. Ezek a fájlok most kimaradnak a profile-ból, és egy
  sor jelzi ezt, pl. `vendor: 312 files skipped (extensions/Facebook, vendors/phpoffice, …)`. A keresések és a
  mappanézetek továbbra is látják őket. A `profile --with-vendor` újra beszámítja őket.
- **A build-kimenet másolatai kimaradnak a profile-ból:** a `dist`, `build` vagy `*.min` nevű mappák (például
  `web/assets.min`) és a minifikált fájlok már nem számítanak kétszer layerként; egy `generated: N files skipped (…)`
  sor mutatja őket.
- **Egy üzleti témáról elnevezett mappa nem event layer:** az `app/Services/Event` vagy egy JavaScript
  `controllers/event` mappa `event` layerré vált. Az `event`, `listener` és `job` szerephez most bizonyíték kell a
  kódban — egy Event / Listener / Subscriber ősosztály vagy interface, a Laravel `Dispatchable` vagy `ShouldQueue`,
  vagy egy `handle(…)` metódus egy `Listeners` namespace-ben; különben a fájl megtartja a másik szerepét (`service`,
  `controller`, illetve `model` az `app/Models/Job.php`-nél).
- **A `.gitignore` által kizárt fájlok már nem jelennek meg:** egy Laravel + Vite projekt build-kimenete
  (`public/build/assets/app-CBW7lKWe.js`, a `.gitignore`-ban `/public/build`-ként) megjelent a `locate`-ben és a
  mappanézetekben. A `.gitignore` most úgy működik, ahogy a git olvassa: `/`-rel kezdődő minták, beágyazott
  útvonalak, `**`, `!` kivételek és bármelyik almappa `.gitignore`-ja. Ugyanezek a szabályok érvényesek mindenhol,
  ahol a callgraph fájlokat néz, az MCP server file watcherét és az editor hookot is beleértve.
- **A minifikált és bundle-ölt JavaScript / CSS akkor is kimarad, ha nincs `.min.` a nevében és nincs
  `.gitignore`:** a rendkívül hosszú első sorokkal kezdődő fájlt generáltnak ismeri fel, és kihagyja.
- **A `locate --orange-500`** (egy CSS-változó) ismeretlen kapcsolóként elutasításra került; most keresi. Az ismert
  kapcsolók (`--all`, `--project`) ugyanúgy működnek, mint eddig.
- **A `callgraph.sh --version`** a `show` használati súgóját írta ki, és hibát naplózott.
- **A szintaktikai hibás fájlok már nem látszanak teljesnek:** egy éppen szerkesztett vagy merge conflictos fájl
  függvények nélkül jöhetett vissza (JavaScript, TypeScript, Vue, Rust), vagy jelzés nélkül (PHP, Swift). Egy keresés
  ilyenkor „no such method”-ot mondott, vagy nem talált hívót, amiből egy agent azt hihette, hogy a kódot nem
  használják. Most a hiba előtti részek megmaradnak (az `a` és `b` függvény látszik, ha a `c` hibás), és minden
  eredmény jelzi ezt: `⚠ syntax error at L3: … — this file's outline may be incomplete` a fájl alatt, egy záró sor,
  pl. `⚠ 2 files had syntax errors (results may be incomplete): src/tail.js L3, src/v.vue L4`, ha egy mappa, egy
  keresés (`--grep`, `--depth`), a `locate` vagy a `profile` hibás fájlt olvasott, `parseErrors` a `--format json`
  kimenetben, és figyelmeztetés a `callers` hibakimenetén. A kilépési kód 0 marad, mert az eredmény többi része
  továbbra is helyes és hasznos. Érvényes a JavaScriptre, TypeScriptre, Vue-ra, PHP-ra, Rustra, Swiftre és JSON-ra
  (a CSS, SCSS és Markdown engedékenyen olvasott, ezeket nem jelzi).

## [0.5.0] — 2026-11-06

### Új
- **Honnan induljak egy feladattal — `locate`:** add meg egyszerre egy feladat neveit és kifejezéseit
  (`callgraph.sh locate RefundController processRefund refund payment_status checkout.vue`, vagy MCP-n az új `locate`
  tool), és kilistázza azt a néhány fájlt, amit érdemes először megnyitni, a legjobbal kezdve, mindegyiket
  indoklással: az ilyen nevű osztály vagy metódus, az ilyen nevű fájl, egy nevet tartalmazó név (`refund` →
  `RefundJob`, `refunds_table`), egy szöveg, például oszlopnév vagy route, vagy egy hívás rá. A projekt-térképből
  válaszol, a kód újraolvasása nélkül, ezért gyors. Az a fájl, amely több kifejezésre is illik, megelőzi azt, amelyik
  egyetlen kifejezésre illik sokszor. 5 fájlt mutat (`--all`: minden találatot), és megmondja, mely kifejezések nem
  találtak semmit, és melyek túl általánosak. A skill és az MCP-útmutató most azt is elmondja az agenteknek, mikor
  használják, és mikor jobb eszköz a sima szöveges keresés (grep).
- **Projekt-profile:** a `callgraph.sh profile` kb. 15 sorban elmondja, hogyan épül fel egy ismeretlen projekt: az
  architektúra (Laravel / Yii layered MVC, DDD / hexagonális, Vue / React frontend, SwiftUI MVVM, Rust web), melyik
  mappa melyik layert tartalmazza (controllerek, service-ek, repositoryk, modellek, komponensek, store-ok…)
  fájlszámmal, az alkalmazott tervezési minták (Repository, Factory, Strategy, Observer…), a szerep nélküli mappák,
  és hogy a layerek a várt irányban hívják-e egymást. A mappa- és fájlneveket olvassa, és a code graph alapján
  ellenőrzi (`extends Controller`, `implements *RepositoryInterface`, `defineStore`); amit nem tud eldönteni, az
  „unknown” marad. A `--full` minden mappát mutat. AI-klienseknek: a `project_profile` MCP tool, és csatlakozáskor egy
  rövid profile.
- **Layerek a lekérdezésekben (opt-in, alapból semmi nem változik):** a `--layer service` csak az adott layer
  fájljaiban keres (nem kell ismerni a mappaneveket), a `--layers` layer szerint csoportosítja a summaryt, és
  minden fájlt megjelöl a szerepével, a `--by-layer` layer szerint rendezi a „ki hívja?” eredményeket, a hívott
  kódhoz legközelebbivel kezdve (a `callers`-nél is, és az MCP toolokon `layer` / `by_layer`-ként). A semmitmondó
  vagy nem angol mappanevek egy commitolható `.callgraph-layers` fájlban rendelhetők hozzá
  (`service = app/Logic`, `ignore-role = app/Legacy`).
- **Kiterjesztések kizárása projektenként:** a projekt gyökerében (vagy bármelyik almappában; a legközelebbi
  érvényes) egy `.callgraphignore` fájl felsorolja, mely kiterjesztéseket hagyja ki ott a callgraph, soronként egyet
  (`md`, `.json` vagy `*.vue`; a `#` kommentet kezd). Commitold, és a csapatban mindenki ugyanazt az eredményt kapja.
  Egy kizárt kiterjesztésű, közvetlenül megnevezett fájlt érthető üzenettel utasít el.

### Változás
- **Csak a támogatott fájlokat dolgozza fel, szigorúbban:** a callgraph eddig is figyelmen kívül hagyta a más
  kiterjesztésű fájlokat; most a generált fájlokat (`package-lock.json`, `*.min.js`, `*.min.css`) és a bináris
  tartalmú fájlokat is kihagyja, akkor is, ha a kiterjesztésük támogatott. Ha egyiket közvetlenül megnevezed,
  eredmény helyett érthető indoklást ad.
- **A nagybetűs kiterjesztés is számít:** a `B.PHP`, `App.TS` vagy `Main.Vue` most ugyanúgy feldolgozódik, mint a
  kisbetűs változata (korábban csendben kimaradt).

### Javítás
- **A symlinken át elért fájl már nem szerepel kétszer:** ha egy symlink a projekten belüli fájlra mutatott, az a
  fájl kétszer jelent meg (és kétszer elemződött) a mappa-eredményekben.
- **Windows: a rövid összefoglaló újra fájlneveket mutat:** amikor egy mappa kimenete túl hosszú volt, és a callgraph
  helyette a fájlonként egysoros összefoglalót mutatta, Windowson minden fájl a teljes útvonalával jelent meg a
  neve helyett.

## [0.4.0] — 2026-11-06

### Új
- **A projekt-térkép előre frissítése:** a `callgraph.sh refresh` azonnal naprakészre hozza a `.callgraph/`
  projekt-térképet — minden változott, új vagy törölt fájlt, vagy csak a megnevezett fájlokat / mappákat —, és egy
  soros összefoglalót ír. Hasznos egy `git pull` után, vagy ha egy agent sok fájlt írt: a következő lekérdezésnek
  már nem kell újraelemeznie őket. A `--background` azonnal visszatér, és külön folyamatban végzi a munkát. Két
  refresh soha nem dolgozik egyszerre ugyanazon a projekten; ha már fut egy, a másik egyszerűen kihagyja.
- **Opcionális Claude Code hook:** miután Claude szerkeszt vagy ír egy fájlt, a callgraph a háttérben frissíti azt,
  így a következő kérdésre azonnal jön a válasz. A telepítő megkérdezi, hozzáadja-e (az `install.sh --hooks`
  kérdés nélkül hozzáadja); csak egy bejegyzést tesz a `~/.claude/settings.json`-ba, minden más beállítást és hookot
  megtart, és előtte `settings.json.bak` mentést készít. Bármikor ellenőrizhető vagy eltávolítható:
  `callgraph.sh hook status` / `callgraph.sh hook uninstall`. Regisztrált projekteken kívül (vagy amelyekben még
  nincs `.callgraph/` mappa) nem csinál semmit.
- **Az MCP server naprakészen tartja magát:** amíg a `callgraph mcp` fut, figyeli a megnyitott projekteket, és a
  változott fájlokat a mentés után egy pillanattal frissíti, így a lekérdezések gyorsak maradnak. A válaszok
  pontosan ugyanazok, mint korábban. Ha egy rendszeren a figyelés nem lehetséges, ezt egyszer jelzi, és úgy működik,
  mint korábban. Kikapcsolás: `CALLGRAPH_WATCH=0`.
- **Az MCP server elmagyarázza magát:** a callgraph-ot MCP-n át használó AI-eszközök (Claude Code, Claude Desktop,
  Cursor, VS Code) csatlakozáskor rövid használati útmutatót kapnak a nyelveden: melyik toolt mikor használják,
  hogyan találják meg egy függvény összes hívóját, és hogyan olvassák a válaszokat. Egy új `guide` tool kérésre a
  teljes kézikönyvet adja. A tool-leírások mindegyike tartalmaz egy példát és a leggyakoribb elkerülendő hibát. Az
  útmutató ugyanabból a szövegből készül, mint a skill `SKILL.md`-je, így a kettő mindig ugyanazt mondja.
- **A telepítő be tudja állítani az MCP servert a Claude Code-hoz:** ha a `claude` parancs elérhető, az
  `install.sh` megkérdezi, regisztrálja-e a callgraph-ot MCP serverként (az `install.sh --mcp` kérdés nélkül
  megteszi, a `--no-mcp` kihagyja). Újrafuttatva nem hoz létre duplikátumot. Kiírja, hogyan távolítható el
  (`claude mcp remove -s user callgraph`), és hol található a Claude Desktop, a Cursor és a VS Code beállítása. Ha ez a
  lépés nem sikerül, a telepítés többi része akkor is lefut. A telepítő a végén emlékeztet a Claude Code
  újraindítására.
- **Az MCP server frissíteni tudja magát:** add hozzá a `--auto-update`-et a server argumentumaihoz az AI-eszközöd
  MCP-beállításaiban (`"args": ["mcp", "--auto-update"]`, ajánlott). Amikor az eszköz elindítja a servert — naponta
  legfeljebb egyszer —, megnézi, van-e újabb callgraph release, letölti, ellenőrzi, hogy a fájl pontosan a közzétett
  (soha nem telepít olyan fájlt, amelynek checksumja nem egyezik), ugyanúgy telepíti, mint a `callgraph.sh update`
  (a projektjeid és beállításaid megmaradnak), és újraindítja magát, így az eszköz azonnal az új verzióval
  dolgozik. Ha bármi elromlik (nincs internet, GitHub-korlát, hibás letöltés, nincs írási jog), egyszerűen a meglévő
  verziót indítja, és egy sort ír az eszköz logjába. Csak a callgraph saját mappájába ír, a projektjeidbe soha.
- **Az MCP server szól a frissítésekről:** `--auto-update` nélkül a server naponta legfeljebb egyszer a háttérben
  megnézi, van-e újabb release (egy kérést sem várakoztat meg), és a session első válaszához egy sort fűz:
  `ℹ callgraph X is available (installed: Y) — update: …/callgraph.sh update, or add --auto-update to the MCP
  config`. A `CALLGRAPH_UPDATE_CHECK=0` minden frissítés-ellenőrzést kikapcsol (semmilyen hálózati forgalom); CI-ben
  is ki vannak kapcsolva.

### Változás
- **A metóduson belüli kommentek a `--body`-ban maradnak:** a rövid metódusnézet a metóduson belüli kommentsorokat is
  elrejtette, így egy agent azt hihette, hogy hiányzik egy sor, és újra beolvasta a fájlt. Most csak a metódus
  feletti docblock és az üres sorok maradnak ki; a metóduson belüli kommentek (gyakran a szándékot magyarázzák)
  látszanak. Ha egy docblock kimaradt, a fejléc sor jelzi, pl. `(docblock L112-119 omitted: --doc)`. A `--doc`
  továbbra is hozzáadja a docblockot, a `--raw` továbbra is a pontos forrást írja ki. Az MCP `method_body` tool
  ugyanígy működik.
- A projekt weboldala saját domaint kapott: https://callgraph.eu (a régi `geriy.github.io/callgraph` cím
  átirányít).

### Javítás
- **A „ki hívja” most megtalálja a leszármazott osztályok hívásait:** egy metódusra az osztálynevével keresve
  (`--grep FrontendController::redirectToNewDashboard`, vagy az MCP `find_callers` tool) kimaradtak azok a helyek,
  ahol egy leszármazott osztály `$this->redirectToNewDashboard()`-ként hívja az örökölt metódust — csak a puszta
  metódusnév találta meg őket. Most ezek a hívások is megjelennek, a leszármazott osztállyal megjelölve
  (`L139 via EventeditController │ …`). Az a leszármazott osztály, amelynek saját változata van a metódusból, nem
  számít, mert az ő hívása a saját változatát futtatja.

## [0.3.0] — 2026-11-06

### Változás
- **Gyorsabb lekérdezések nagy projekteken:** a callgraph most egy tömör projekt-térképet tart a `.callgraph/`
  mappában, mindössze három fájlban, akármekkora is a projekt: az összes fájl rövid listája, egy fájl minden fájl
  teljes elemzésével, és egy az összefoglalóval, amire a kereséseknek szükségük van. Fájlonként frissül, csak az
  utolsó lekérdezés óta változott fájloknál (a törölt vagy átnevezett fájlok kikerülnek). A „hol hívják”
  (`--grep`, minden `--depth` szint), a `callers`, a `--strings` és a `--const` egyetlen olvasással az
  összefoglaló fájlból válaszol; egy egyfájlos nézet (outline, `--body` …) csak az adott fájl részét olvassa a teljes
  elemzésből. A kimenet pontosan ugyanaz, mint korábban. Egy szerkesztett fájl már a következő lekérdezésben
  megjelenik; bármi sérült vagy más callgraph-verzió által írt csendben újraépül. A térkép a régi elemzési cache-t
  (`cache.jsonl`) váltja, amely automatikusan törlődik, így a mappa kevesebb helyet foglal, mint korábban. Az MCP
  server (`callgraph mcp`) a kérések között memóriában tartja a térképet. Kikapcsolás (pl. összehasonlításhoz):
  `CALLGRAPH_NO_GRAPH=1`. Részletek: `docs/PROJECT-GRAPH.md`.
- **Rövidebb hívólisták a „hol hívják” eredményekben:** ha egy hívó metódus több soron is ugyanazt a hívást teszi,
  pontosan ugyanazzal a forrásszöveggel, most egyszer jelenik meg az összes sorszámmal, pl.
  `L12, L40 │ $this->repo->find($id)`. Hívónként legfeljebb 3 ilyen sor látszik; a többi csak sorszámmal:
  `… +2 more: L55, L61`. Érvényes a `--grep`-re, az első `--depth` szintre és a hívásonként egysoros nézetre, amit
  túl hosszú eredménynél használ. Az interface-en át történő hívások külön sorban maradnak (`via …`).
- **Rövidebb metódus-törzsek:** a `--body` most kihagyja, amire egy agentnek ritkán van szüksége: a metódus feletti
  docblockot / doc kommenteket, az üres sorokat és a csak kommentből álló sorokat. A kódsorok pontosan úgy jelennek
  meg, ahogy írva vannak (a kódsor végén álló komment megmarad), és egy string, heredoc vagy template literal
  belsejéből soha nem töröl semmit. A sorszámok továbbra is a valódiak, így egy hézag mutatja, hol maradtak ki sorok,
  és továbbra is kiolvasható egy pontos tartomány. Az olyan attribútumok, mint `#[test]` / `#[Route(...)]`,
  megmaradnak. Működik PHP-ra, JS / TS / Vue-ra, Rustra és Swiftre, egyszerre több metódusra, és a
  `--grep … --body`-ra is.
- Az új `--doc` visszaadja a docblockot; az új `--raw` a pontos, szűretlen forrást írja ki (mint korábban, plusz a
  Rust / Swift doc kommenteket és attribútumokat). Az MCP `method_body` tool ugyanezt az alapértelmezést követi, és
  `doc` / `raw` opciói vannak.

### Új
- **Jegyzetek fájlokhoz és metódusokhoz (kísérleti):** a `callgraph note add <file>[::<method>] "<szöveg>"` (vagy
  `callgraph.sh note …`) rövid jegyzetet csatol, a forráson kívül tárolva a `<project>/.callgraph/notes.jsonl`-ben
  (git-ignored), dátummal és idővel (UTC). Az outline a fájl fejléce vagy a metódus alatt mutatja
  (`📝 2026-11-05 20:27 · szöveg`, a legújabb elöl, legfeljebb 3), a `--body` a metódus forrása felett. A
  `note list [<path>]` listázza őket (`[orphaned]` jelöléssel, ha a metódus már nincs meg), a `note rm <id>` töröl
  egyet, a `show <dir> --notes <minta>` keres bennük. A horgony a metódus neve, nem egy sorszám. MCP: egy új,
  csak olvasható `notes` tool (írás tool nincs). Jegyzetek nélkül a kimenet változatlan. Részletek: `docs/NOTES.md`.
- **TypeScript: hívások típusos objektumokon és interface-eken át.** A callgraph most kikövetkezteti, melyik
  osztályhoz megy egy TS metódushívás, ha a típus le van írva: típusos paraméter (`repo: Repo`), típusos
  osztályproperty (`private repo: Repo`, a `constructor(private repo: Repo)` is), `this.repo = new Repo()` vagy
  `const x = new Repo()`. Így a `this.repo.find()` `Repo::find`-ként jelenik meg, és a `--grep 'Repo::find'`
  megtalálja. Az osztályok rögzítik az `implements`-üket, és a TS interface-ek (`interface X extends Y`) megjelennek a
  térképen, így a `PlaylistResolver::getPlayables` keresése a `Resolver` interface-en át történő hívásokat is
  megtalálja, `via Resolver` jelöléssel (a `--depth`-ben és a `callers`-ben is). Ha a típus nem egyértelmű (nincs
  típus, `any`, egy union, mint `Repo | null`, egy generic `T`, vagy különböző értékek ugyanabban a függvényben), a
  hívás marad a régiben.
- **A `--const` a re-exportokat is listázza:** a hivatkozások és a definíciók mellett a `--const NAME` most azt is
  mutatja, hol adják tovább a nevet, `[re-export]` jelöléssel: Rust `pub use state::{CheckState, STATE_FILE};` /
  `pub(crate) use …`, JS / TS `export { NAME } from './x'` / `export { NAME }`, PHP `use const App\NAME;`.

### Javítás
- A `--grep <minta> --body` újra kiírja a hívó metódusok forrását (az opciót figyelmen kívül hagyta).
- MCP `method_body`: a `method` egyetlen név is lehet sima stringként, nem csak lista (egyes kliensek
  `"method": "parse"`-t küldenek, ami korábban „expected a sequence” hibával elbukott).

### Javítás
- **JS/TS: nincs több duplán listázott metódus, ha egy osztály neve megegyezik a fájléval** (`PlaylistResolver.ts`
  `class PlaylistResolver`-rel): az osztály metódusai kétszer szerepeltek, egyszer a fájl `module` bejegyzése alatt,
  egyszer az osztály alatt. Most az osztálytagok csak az osztály alatt, a legfelső szintű függvények csak a modul
  alatt jelennek meg.

## [0.2.0] — 2026-11-05

### Új
- **Háttérben futó frissítés-ellenőrzés:** a `callgraph.sh` naponta legfeljebb egyszer, egy leválasztott háttér-
  folyamatban megnézi a GitHubot (soha nem lassít le egy lekérdezést). Ha van újabb release, a kimenet egy sorral
  zárul, naponta egyszer: `ℹ callgraph X is available (installed: Y) — update: bash …/callgraph.sh update`. A Claude
  Code szól a felhasználónak, és felajánlja a frissítést; magától soha nem frissít. Kikapcsolás:
  `CALLGRAPH_UPDATE_CHECK=0`; CI-ben ki van kapcsolva.
- **Hibajelentés GitHubon át:** kétnyelvű issue-űrlapok (hibajelentés, ötlet / visszajelzés), egy privát security
  report link és `SECURITY.md`. A hibaüzenetek most a hibajelentő űrlapra mutatnak; a
  `feedback.sh show <error-id>` kiírja a bemásolandó részleteket.
- **Angol skill-útmutató:** a `SKILL.md` most angolul és magyarul is megvan (`skill/skill-i18n/SKILL.<en|hu>.md`). A
  skill követi a nyelvet: a `callgraph.sh set-lang <en|hu>` az adott nyelven írja újra a `SKILL.md`-t (a másik
  eltűnik); az első telepítés (`install.sh` → új `callgraph skill-sync`) és az `update` megtartja a felhasználó
  nyelvét. A skill újratöltéséhez indítsd újra a Claude Code-ot.
- **Release checksumok:** minden release-nek van `SHA256SUMS` assetje (`sha256sum` formátum, minden asset). Az
  `update` telepítés előtt ellenőrzi a letöltött zip SHA-256-ját, és eltérés esetén semmit nem telepít; a
  `SHA256SUMS` nélküli release-t (v0.1.0) elutasítja, hacsak nincs megadva az `--insecure-skip-checksum`. Az
  `update --from <zip>` a `--sha256 <hex>`-et vagy a zip melletti `SHA256SUMS`-ot ellenőrzi, különben egy sorban
  figyelmeztet, és telepít.
- **MCP server mód:** a `callgraph mcp` egy Model Context Protocol servert futtat stdio-n (a hivatalos Rust SDK, `rmcp`),
  így a Claude Desktop, a Cursor, a VS Code vagy bármely MCP-kliens használhatja a callgraph-ot. Toolok: `outline`,
  `method_body`, `find_callers`, `search`, `framework_view`, `projects` — ugyanaz a tömör szöveg és kimeneti korlát,
  mint a CLI-nél. Csak olvas, nincs hálózat, az útvonalak a projekt gyökerére korlátozva. A skill / CLI használata
  változatlan. Beállítási receptek: `docs/MCP.md`.
- **Látszik, hogyan hívják:** a „hol hívják” eredmények (`--grep` és a `--depth` minden szintje) most a sorszám
  mellett a hívás saját forrássorát is mutatják, pl. `L25 │ $this->repo->find($id)` — nem kell külön `--body`
  hívás csak azért, hogy lásd az argumentumokat.
- **Hívások interface-eken és traiteken át:** egy osztálymetódus keresése (`--grep 'PlaylistResolver::getPlayables'`)
  az interface-én, absztrakt szülőjén vagy traitjén át történő hívásokat is megtalálja (és fordítva, a felül nem
  írt leszármazottakon át történőket is). Ezek meg vannak jelölve, pl. `L14 via Resolver │ …`; a `--depth` követi
  őket a következő szintre, a `callers` pedig egy `"via"` mezőt ad hozzá. Ott működik, ahol a hívás receiverének típusa
  ismert (PHP: típusos propertyk és más feloldott receiverek; Rust: `Type::method` hívások); a JS/TS interface-eket és
  a Swift protocolokat még nem követi.
- **Rust: okosabb metódushívások.** Az `x.load()`-nál a callgraph most az egyszerű esetekben kikövetkezteti az `x`
  típusát (paramétertípus, `let x = Repo::new()`, `Repo { … }`, `self`, a saját struct egy mezője, mint a
  `self.repo`), így a hívás `Repo::load`-ként jelenik meg, és a „ki hívja a `Repo::load`-ot?” megtalálja. Ha a típus
  nem következtethető ki, a hívás `?::load`-ként marad meg, és egy `Repo::load` keresés bizonytalanként megjelölve
  akkor is listázza — ahelyett, hogy csendben kihagyná.
- **Rust konstansok:** a `--const` most megtalálja a Rust `const` / `static` elemeket — hol vannak definiálva és hol
  használják őket (a `Type::CONST`-ot is).
- **JS / TS objektum-metódusok:** az object literalon belüli függvények (`const api = { fetch() {}, save: () => {} }`,
  vagy egy sima `export default { … }`) külön metódusként szerepelnek (`api.fetch`) a sortartományukkal, és a rájuk
  irányuló hívások is megtalálhatók.

### Változás
- **Gyorsabb build:** a `tools/dev.sh check` / `test` / `clippy` a debug profile-t használja (mint most a CI; a
  `check-release` megtartja az LTO-s változatot), a dev konténer egyetlen build cache-t oszt meg minden checkout
  között (`CALLGRAPH_DEV_TARGET`), a CI pedig cache-eli a lefordított függőségeket (`Swatinem/rust-cache`), és LTO
  nélkül futtatja a clippyt és a teszteket. A kiadott binaryk pontosan úgy épülnek, mint korábban (release profile,
  LTO).
- **Rövidebb találati listák:** ha a projekt gyökere sok sorban ismétlődne (`--grep`, `--depth`, `--strings`,
  `--const`, mappa-outline-ok, framework nézetek…), az első sor most `root: /abs/path/to/project`, és utána minden
  útvonal ehhez a gyökérhez relatív. Az egyetlen útvonalat tartalmazó kimenetek és a gépi formátumok (`json`,
  `mermaid`, `dot`, `callers`) változatlanok.
- A commitolt / csomagolt alapértelmezett `SKILL.md` most angol (a `skill-i18n/SKILL.en.md`-ből generálva).
- A `SKILL.md` most a név szerinti hívókeresést ajánlja (`--grep '(^|::|\.|->)name$'`), ami az örökölt, traites és
  ismeretlen receiverű hívásokat is elkapja; a `Type::method` egy gyakori név szűkítésére való. Elmagyarázza az új
  keresési kimenetet is (`root:` sor, `L<n> │ code`, `via`, `?::`) és az új Rust / JS opciókat.
- A `--const` a megfelelő konstansdefiníciókat is listázza (`[definition]` jelöléssel), nem csak a hivatkozásokat.
- Az `update` / `update --check` verziókeresése a GitHub API helyett a `releases/latest` átirányítást használja, így
  az API rate limit (HTTP 403) már nem érvényes rá.
- A projektoldal a `gh-pages` ágra költözött (https://geriy.github.io/callgraph/); az oldal szerkesztése már nem érinti
  a `main`-t.

### Javítás
- Windows: a projekt gyökere `C:\…`-ként jelenik meg a keresési kimenetben a `\\?\C:\…` helyett.
- Egy puszta metódus-selector egy object literal metódusát is megtalálja: a `store.ts::updateSongs` a
  `playableStore.updateSongs`-ot választja ki.
- `--grep … --depth`: a 2. szinttől a hívókat az egész projektben keresi (a projekt gyökerétől), nem csak az
  1. szint találatainak mappáiban — a más mappákban lévő hívók kimaradtak.
- A Rust `--format api` most listázza az `impl` blokkok (és trait-implementációk) `pub fn` metódusait, a `pub`
  konstansokat és mezőket — korábban a Rust típusoknál nem mutatott publikus metódust.
- A `--strings` most megtalálja a Vue template-kifejezésekben lévő szövegeket (`{{ … }}`, `:title="…"`,
  `@click="…"`) és a Rust `const` / `static` értékekben lévőket, a byte stringeket (`b"…"`) is beleértve.
- SCSS: a `@mixin` / `@function` paraméterek (`$color: t.$brand, …`) és az `@include` kulcsszavas argumentumok
  konstansként jelentek meg; most csak a `$var: value;` deklarációk (a paraméterek a mixin paraméterei maradnak).
- Az `update --check` most az eredményét az `update-check.json`-ba is beírja, így a napi háttér-ellenőrzés nem
  ismétli meg, és a kézzel talált újabb verzió megjelenik az értesítésben.

## [0.1.0] — 2026-11-05

Első kiadás.

- Nyelvek: PHP, JavaScript / TypeScript, Vue, **Rust**, **Swift / SwiftUI**, CSS / SCSS, JSON, Markdown.
- Szerkezet, kategorizált call graph, metódus-törzsek, „ki hívja?” és hatáselemzés (`--grep --depth`),
  framework nézetek (Laravel / Yii), sokféle kimeneti formátum.
- Angol vagy magyar kimenet (`callgraph.sh set-lang en|hu`).
- Beépített frissítés: `callgraph --version`, `callgraph.sh update [--check|--force]`.
- Előre fordított csomagok macOS-re (arm64, x86_64), Linuxra (x86_64, ARM64, statikus musl) és Windowsra.

[Unreleased]: https://github.com/GeRiY/callgraph/compare/v1.2.1...develop
[1.2.1]: https://github.com/GeRiY/callgraph/releases/tag/v1.2.1
[1.2.0]: https://github.com/GeRiY/callgraph/releases/tag/v1.2.0
[1.1.1]: https://github.com/GeRiY/callgraph/releases/tag/v1.1.1
[1.1.0]: https://github.com/GeRiY/callgraph/releases/tag/v1.1.0
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
