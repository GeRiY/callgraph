# Változásnapló

A callgraph minden fontos változása. A munka csomagokban gyűlik a `develop` ágon, és együtt kerül kiadásra.
Formátum: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), verziók: [SemVer](https://semver.org/).
Angol változat: [CHANGELOG.md](CHANGELOG.md) — a kettő tartalma azonos.

## [Unreleased]

### Új
- **`dynamic_call` jelölés:** a futásidőben eldőlő célú hívások eddig hiányoztak az outline-ból, így a gráf ott is
  teljesnek látszott, ahol nem az. Mostantól szerepelnek (a többi kategória után, a lista végén), célnak a hívott
  kifejezéssel — PHP: `$obj->$m()`, `$obj->{$expr}()`, `$class::$m()`, `$fn()`, `new $class`, `call_user_func($x)`,
  `app($var)` / `resolve($var)` / `$this->app->make($var)`; JS / TS / Vue: `obj[expr]()`, `new (expr)()`,
  `import(expr)`, `require(expr)`; Python: `getattr(obj, név)()`, `globals()[név]()`, `importlib.import_module(név)`,
  `eval` / `exec`. A literál esetek (`obj['x']()`, `app(Foo::class)`, `getattr(obj, 'x')()`) közönségesek maradnak.
  Jelölés, nem hívó-él: a `--grep`, a `--depth` és a `locate` figyelmen kívül hagyja; a `--depth` egy sorban jelzi,
  hány hívó tartalmaz dinamikus hívást („kézzel nézd át”). Szűrés: `--category dynamic_call`. Rust, Swift, Go: még nincs.

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

[Unreleased]: https://github.com/GeRiY/callgraph/compare/v0.8.0...develop
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
