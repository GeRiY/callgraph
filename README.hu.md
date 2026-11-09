<h1 align="center">callgraph</h1>

<p align="center">
  <b>Token-hatékony kód-térkép AI kódoló agenteknek.</b><br>
  Szerkezet, kategorizált hívás-gráf, metódus-törzs, „ki hívja?” és hatáselemzés —
  a teljes fájlok beolvasása nélkül.<br>
  🌐 <a href="https://callgraph.eu/hu/">callgraph.eu</a><br>
  Útmutatók: <a href="https://callgraph.eu/hu/mcp/">MCP-beállítás</a> · <a href="https://callgraph.eu/hu/php/">PHP</a> · <a href="https://callgraph.eu/hu/rust/">Rust</a> ·
  <a href="https://callgraph.eu/hu/typescript/">TypeScript / Vue</a> · <a href="https://callgraph.eu/hu/swift/">Swift</a> · <a href="https://callgraph.eu/hu/locate/">locate</a> ·
  <a href="https://callgraph.eu/hu/profile/">Projekt-profil</a> · <a href="https://callgraph.eu/hu/changelog/">Változásnapló</a>
</p>

<p align="center">
  <a href="https://github.com/GeRiY/callgraph/actions/workflows/release.yml"><img alt="build" src="https://github.com/GeRiY/callgraph/actions/workflows/release.yml/badge.svg?branch=main"></a>
  <a href="https://github.com/GeRiY/callgraph/releases/latest"><img alt="release" src="https://img.shields.io/github/v/release/GeRiY/callgraph?display_name=tag&sort=semver"></a>
  <a href="LICENSE"><img alt="licenc: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <img alt="platformok" src="https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey">
</p>

<p align="center">
  <a href="#gyors-kezdés">Gyors kezdés</a> ·
  <a href="#telepítés">Telepítés</a> ·
  <a href="#használat">Használat</a> ·
  <a href="#hogyan-működik">Hogyan működik</a> ·
  <a href="#fejlesztés">Fejlesztés</a> ·
  <a href="README.md">English</a>
</p>

---

Ha egy agentnek egyetlen metódust kell megértenie, többnyire beolvassa a teljes fájlt, aztán grepel, aztán még
néhány fájlt elolvas. A **callgraph** ezekre a kérdésekre az AST-ből válaszol, pár tucat sorban: mi van a fájlban,
melyik metódus melyik sorokon él, mit hív (kategóriánként), ki hívja, és mi romlik el, ha megváltozik.

Egyetlen natív Rust-bináris és egy vékony [Claude Code](https://claude.com/claude-code) skill; bármelyik terminálból
vagy szkriptből is ugyanígy használható.

```console
$ callgraph.sh --project shop app/Models/Order.php
FILE  /srv/shop/app/Models/Order.php  NS:App\Models

class Order extends Illuminate\Database\Eloquent\Model  L7-23
  constants (1):
    STATUS_OPEN = 'open'
  properties (1):
    mixed $fillable
  saveTranslations()  L13-20
    external_call     create, info
    global_helper     logger
    internal_helper   translations
  translations()  L22-22
    orm_relation      hasMany
```

```console
$ callgraph.sh --project shop app --grep saveTranslations --depth 2
== 1. szint — közvetlenül hívja: /saveTranslations/
  /srv/shop/app/Models/Order.php  Caller::run()  L25-25

== 2. szint — az előző szintet hívja
  (nincs további hívó)
```

## Fő jellemzők

- **Az AST-t olvassa, nem a szöveget.** Osztályok / komponensek, metódusok pontos sortartománnyal, property-k,
  konstansok, importok, docblockok, string-literálok és `if` / `match` / `switch` ágak.
- **Kategorizált hívás-gráf.** Minden hívás besorolást kap: `native_php`, `orm_relation`, `model_use`, `global_helper`,
  `internal_helper`, `external_call`, `framework_use`, Vue `composable_use` és még több — az agent a szándékot látja,
  nem csak neveket.
- **„Ki hívja?” és hatáselemzés.** A `--grep <minta> --depth N` a hívók hívóit járja be az egész projektben, cache-elt,
  inkrementális projektindexszel.
- **Célzott olvasás.** A `fájl::metódus --body` csak azt a metódust írja ki, sorszámozva; a `--lines 30-60` bármely sávot.
- **Framework-nézetek.** Laravel / Yii ismeret: `routes`, `middleware`, `validation`, `jobs`, `events`, `providers`,
  `magic`, `di`, `views`.
- **Sokféle kimenet.** `outline`, `api`, `flow`, `summary`, `hotspots`, `coverage`, `stats`, `mermaid`, `dot`,
  `json`, `jsonl`.
- **Gyors és függőségmentes.** Egy statikus bináris (nem kell PHP, Node vagy jq), párhuzamos elemzés, mtime-alapú JSONL cache.
- **Angol vagy magyar kimenet**, egy paranccsal váltható.

### Támogatott nyelvek

Nyelvenkénti útmutatók példákkal: [PHP](https://callgraph.eu/hu/php/) · [Rust](https://callgraph.eu/hu/rust/) · [TypeScript / JavaScript / Vue](https://callgraph.eu/hu/typescript/) · [Swift / SwiftUI](https://callgraph.eu/hu/swift/).

| Nyelv | Kiterjesztések | Mit kapsz |
|---|---|---|
| PHP | `.php` | osztályok, traitek, enumok, metódusok, property-k, konstansok, hívások típuskövetéssel, framework-felismerés (Laravel, Yii) |
| Rust | `.rs` | modulok, structok, enumok, traitek, `impl`-blokkok, függvények láthatósággal / `async`-kal, mezők, konstansok, `use`-fák, útvonal- / metódus- / makróhívások, `if` / `match` ágak, framework-felismerés a `Cargo.toml`-ból (axum, actix-web, tokio, serde, …) |
| JavaScript / TypeScript | `.js` `.mjs` `.cjs` `.jsx` `.ts` `.tsx` | modulok, függvények, osztályok, importok alias-feloldással (`jsconfig` / `tsconfig`) |
| Vue | `.vue` | props, emits, `setup`, template-komponensek és eseménykezelők, `computed`, `watch`, életciklus-hookok, Options API |
| Swift / SwiftUI | `.swift` | osztályok, structok, enumok, protokollok, actorok, extensionök; a `View` típusok komponensként; `func` / `init` / computed property-k (`body`); property wrapperek (`@State`, `@Binding`, `@Published`, …); a SwiftUI view-k és modifierek saját hívás-kategóriával |
| CSS / SCSS | `.css` `.scss` | szabály-blokkok és at-szabályok (`@media`, `@layer`, Tailwind v4 `@theme` / `@utility` / `@source` …), mixinek, függvények, változók, `@use` / `@import` |
| JSON / Markdown | `.json` `.md` | kulcs-útvonalak a rövid értékükkel (`package.json::scripts --body` egy kulcsot ír ki); címsor-szakaszok szintenként, és linkek |

### Mely fájlokat dolgozza fel

Csak a fenti kiterjesztésű fájlokat, kis- és nagybetűvel is (a `B.PHP` is PHP); minden mást figyelmen kívül hagy.
Támogatott kiterjesztéssel is kimarad: a generált fájl (`package-lock.json`, `*.min.js`, `*.min.css`) és a bináris
tartalmú fájl. A minifikált vagy bundlolt JavaScriptet / CSS-t a tartalma alapján is felismeri (nagyon hosszú sorok,
pl. egy Vite-os `app-CBW7lKWe.js`), és kihagyja. Minden fájlt egyszer olvas, akkor is, ha egy symlink is mutat rá. A
`vendor/` és a `node_modules/` mappa kimarad, és minden, amit a `.gitignore` fájljaid kizárnak, a git szabályaival:
`/public/build`, `public/build/`, `build/`, `**/assets`, `*.log`, `!keep.js`, és minden almappa `.gitignore`-ja a
saját mappájához képest.

Ha egy projektben még mást is kizárnál, tegyél egy `.callgraphignore` fájlt a gyökerébe (vagy bármelyik almappába).
Commitold, és az egész csapatnál ugyanaz lesz az eredmény. Az a sor, amelyik csak egy kiterjesztés (`md`, `.md` vagy
`*.md`), azt a kiterjesztést hagyja ki (a legközelebbi `.callgraphignore` érvényes, mint eddig); minden más sor
`.gitignore`-szerű útminta, a fájl mappájához képest:

```
# callgraph: ezeket a kiterjesztéseket hagyja ki ebben a projektben
md
json
*.vue
# és ezeket az utakat (a .gitignore szintaxisával)
public/build/
/storage/
legacy/**/*.php
```

Ha a `.gitignore` által kizárt fájlokat is fel akarod térképezni, add hozzá bármelyik parancshoz a `--no-gitignore`
kapcsolót (az MCP-szervernél és a hooknál: `CALLGRAPH_GITIGNORE=0`); a `.callgraphignore` és a beépített kihagyott
mappák ekkor is érvényesek. Figyelem: a `.callgraphignore`-ban egy puszta szó (pl. `build`) kiterjesztés-szabály;
mappához írd így: `build/` vagy `/build`.

### Mikor *ne* használd

- Egy ismert sorsáv pontos szövege kell: `--lines` vagy célzott olvasás.
- Nem kód fájlok (sima szöveg, adat-dump): az outline itt keveset ad.
- Makró- vagy reflexió-nehéz kód: a statikusan nem feloldható, dinamikus hívások csak név szerinti egyezésként (`?`,
  `--guess`-szel) jelennek meg, vagy sehogy.
- Szintaktikai hibás fájl (szerkesztés közben, merge-konfliktus): `⚠ szintaktikai hiba itt: L<n>` és egy záró `⚠` sor
  jelzi; a hiba előtti rész a térképen van, a többi hiányozhat — az ilyen fájlt olvasd közvetlenül.

## Gyors kezdés

```bash
# 1. telepítés (macOS / Linux; Windows: lent)
p="$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)" && t="$(mktemp -d)" \
  && curl -fsSL -o "$t/cg.zip" "https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-$p.zip" \
  && mkdir -p ~/.claude/skills && unzip -qo "$t/cg.zip" -d ~/.claude/skills \
  && bash ~/.claude/skills/callgraph/install.sh && rm -rf "$t"

# 2. projekt regisztrálása (egyszer)
bash ~/.claude/skills/callgraph/projects.sh add shop /abszolút/út/a/shophoz

# 3. kérdezz
CG=~/.claude/skills/callgraph/callgraph.sh
bash $CG --project shop app/Models/Order.php                      # outline
bash $CG --project shop app/Models/Order.php::save --body         # egy metódus forrása
bash $CG --project shop app --grep 'Order::cancel' --depth 3      # hatáselemzés
bash $CG set-lang hu                                              # magyar kimenet
```

Utána indítsd újra a Claude Code-ot, hogy betöltse a skillt (`SKILL.md`).

## Telepítés

A kész csomagok (skill-fájlok + a platform binárisa + telepítési útmutató) minden
[GitHub Release](https://github.com/GeRiY/callgraph/releases/latest)-hez csatolva vannak:

| Platform | Csomag |
|---|---|
| macOS, Apple Silicon (M1–M4) | [callgraph-darwin-arm64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-darwin-arm64.zip) |
| macOS, Intel | [callgraph-darwin-x86_64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-darwin-x86_64.zip) |
| Linux x86_64 (statikus musl) | [callgraph-linux-x86_64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-linux-x86_64.zip) |
| Linux ARM64 (statikus musl) | [callgraph-linux-aarch64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-linux-aarch64.zip) |
| Windows x86_64 (Git Bash) | [callgraph-windows-x86_64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-windows-x86_64.zip) |

Mind az öt bináris egy archívumban: `callgraph-<verzió>.tar.gz` ugyanazon a release-oldalon.

**macOS / Linux:** a [Gyors kezdés](#gyors-kezdés) egysorosa. A `~/.claude/skills/callgraph/` mappába telepít, és
lefuttatja az `install.sh`-t. A frissítés ugyanez a parancs; a `projects.json`, az `errors.jsonl`, a `feedback.jsonl`
és a nyelvbeállítás megmarad.

**Windows / kézzel:** csomagold ki a zipet, a benne lévő `callgraph/` mappát másold a `~/.claude/skills/` alá
(Windows: `%USERPROFILE%\.claude\skills\`), és futtasd ott a `bash install.sh`-t (Git Bash). macOS-en az `install.sh`
a letöltés karantén-jelzőjét is leveszi. Lépésenkénti útmutató: [`skill/TELEPITES.md`](skill/TELEPITES.md).

**Forrásból:** lásd [Fejlesztés](#fejlesztés).

Futtatáshoz csak `bash` kell.

### Frissítés

```bash
bash ~/.claude/skills/callgraph/callgraph.sh update --check   # van-e újabb kiadás? (nem változtat semmit)
bash ~/.claude/skills/callgraph/callgraph.sh update           # letöltés és telepítés
```

A frissítés a legújabb GitHub Release-ből letölti a platformod zipjét, és felülírja a skill-fájlokat és a `bin/`-t;
a `projects.json`, az `errors.jsonl`, a `feedback.jsonl` és a nyelvbeállítás megmarad. `--force`: a jelenlegi verzió
újratelepítése; `callgraph --version`: a telepített verzió.
Minden kiadásban van egy `SHA256SUMS` fájl; a frissítés ehhez veti össze a letöltött zipet, eltérésnél semmit nem
telepít. Az ilyen fájl nélküli kiadást (v0.1.0) csak `--insecure-skip-checksum`-mal telepíti. `--from <zip>` esetén a
`--sha256 <hex>` vagy a zip melletti `SHA256SUMS` szerint ellenőriz (ha egyik sincs: egysoros figyelmeztetés).
A `callgraph.sh` naponta legfeljebb egyszer a háttérben (a lekérdezéseket nem lassítva) megnézi, van-e új kiadás, és ha
igen, a kimenete egy `ℹ callgraph X elérhető` sorral végződik; kikapcsolás: `CALLGRAPH_UPDATE_CHECK=0` (a `CI`
beállítása mellett sem fut).

**Dev-buildek (develop ág):** az `update --dev` a `develop` ág legújabb buildjét telepíti (a folyamatosan frissülő
`dev` előzetes kiadás, verziója `X.Y.Z-dev.<commit>`, a kiadásokhoz hasonlóan a `SHA256SUMS` szerint ellenőrizve); az
`update --check --dev` csak megmondja, van-e újabb. A dev-buildek a még ki nem adott változások kipróbálására valók,
lehetnek instabilak. A sima `update` visszavisz a legutóbbi stabil kiadásra; a háttér-ellenőrzés és az MCP
`--auto-update` a stabil kiadásoknál marad (dev-buildnél csak egy újabb stabil verzióról szól).

## Használat

Minden parancs a `callgraph.sh`-n át megy (vagy közvetlenül a binárissal: `callgraph show …`). A `--project <alias>`
mellett az utak a regisztrált projekt gyökeréhez képest értendők; nélküle az aktuális mappa a gyökér.

| Feladat | Parancs |
|---|---|
| Honnan induljak ezzel a feladattal? | `callgraph.sh locate <kifejezés>... [--all]` |
| Fájl / mappa outline-ja | `callgraph.sh <fájl\|mappa>` |
| Csak egy metódus | `callgraph.sh <fájl>::<metódus>` (több szelektor is mehet) |
| Metódus forrása sorszámozva | `callgraph.sh <fájl>::<metódus> --body` |
| Tetszőleges sorsáv | `callgraph.sh <fájl> --lines 30-60` |
| Hol hívják? | `callgraph.sh <mappa> --grep <minta>` |
| Hatáselemzés | `callgraph.sh <mappa> --grep <minta> --depth N [--guess]` |
| String-literál / konstans helye | `callgraph.sh <mappa> --strings <minta>` · `--const <minta>` |
| Szűrés hívás-kategóriára | `callgraph.sh <mappa> --category model_use,orm_relation` |

### Honnan induljak: `locate`

Bővebben a weboldalon: [callgraph.eu/hu/locate](https://callgraph.eu/hu/locate/).

Add át a `locate`-nek a feladat neveit és fogalmait egy hívásban (függvény-, osztály- és fájlnevek, oszlopnevek,
útvonalak); a projekt-térképből válaszol, a forrás beolvasása nélkül: rövid, rangsorolt lista arról, melyik fájllal
kezdj, mindegyik az indoklásával:

```bash
bash ~/.claude/skills/callgraph/callgraph.sh locate RefundController processRefund refund payment_status checkout.vue --project myapp
```

```
gyökér: /home/en/myapp
locate: 5 kifejezés · 14 találat 6 fájlban · innen indulj ↓
1. app/Http/Controllers/RefundController.php  ★★★
   class RefundController  L6-16  ← "RefundController", "refund" (pontos class)
   store()  L14  hívja: RefundService::processRefund  ← "processRefund" (hívás)
2. app/Services/RefundService.php  ★★
   processRefund()  L6-9  hívók: 2 (RefundController::store, RefundJob::handle)  ← "processRefund", "refund" (pontos metódus)
…
```

Az egyezés annál többet ér, minél erősebb: pontos definíció, aztán fájlnév, aztán egy név része (`refund` ~
`RefundJob`, `refunds_table`, egyes / többes szám), aztán string-literál, végül hívás. Az a fájl, amelyik több
különböző kifejezésre illik, előrébb kerül, mint amelyik egyre sokszor, és a pontos definíciójú vagy fájlnevű
fájlok mindig megelőzik a csak stringgel illőket (fordítások, build-kimenet). Alapból 5 fájl látszik (`--all`: minden
találat); a végén a találat nélküli és a túl általános kifejezések. Az utak mindig a `gyökér:` sorhoz relatívak. A
sima szövegkeresés (pontos üzenet, konfig-érték) továbbra is a grep dolga; a `locate` arra felel: honnan induljak?

Különleges kifejezések:

- **Több szavas kifejezés**: a szóközös, idézőjeles argumentum (`"Szervező további eseményei"`) egyetlen kifejezés.
  Egészében illeszti (kis- / nagybetűtől függetlenül) a string-literálokra, pl. feliratokra és fordításokra; szavakra
  sosem bontja.
- **Útvonalnév**: a `route::events.show` elöl az útvonal definícióját adja (`->name('events.show')` a
  `routes/*.php`-ban), utána minden helyet, ahol a nevet használják: `route('events.show', …)`, `to_route(…)`,
  `redirect()->route(…)` a PHP-ban, és `route('events.show')` a JS / TS / Vue-ban (Ziggy). Ha nincs találat, kiírja,
  mit keresett.
- **CSS custom property**: a `locate --orange-500` a változót keresi (`--` elválasztó nélkül is); az ismert kapcsolók
  (`--all`, `--project`) kapcsolók maradnak.

```bash
bash ~/.claude/skills/callgraph/callgraph.sh locate route::events.show "Szervező további eseményei" --project myapp
```

### Kimeneti formátumok (`--format`)

| Formátum | Mit ad |
|---|---|
| `outline` (alap) | szerkezet + hívások kategóriánként |
| `api` | publikus szerződés + docblockok, implementáció nélkül |
| `flow` | `if` / `match` / `switch` ágak sortartománnyal (feltétel → mit hív) |
| `summary` | fájlonként egy sor |
| `hotspots` | top 20 metódus be-/kifok szerint |
| `coverage` | dokumentálatlan publikus metódusok |
| `stats` | kategória-statisztika |
| `mermaid` / `dot` | öröklési és hívási élek diagramként |
| `json` / `jsonl` | a nyers gráf |
| `routes` `middleware` `validation` `jobs` `events` `providers` `magic` `di` `views` | framework-nézetek (mappára futtasd) |

A nagy eredmények korlátosak (400 sor fölött a mappás outline summary-ra, az egyfájlos metódus-indexre vált);
a `--full` feloldja. Teljes referencia: `callgraph.sh --help` és [`skill/SKILL.md`](skill/SKILL.md).

### Projekt-profil

Bővebben a weboldalon: [callgraph.eu/hu/profile](https://callgraph.eu/hu/profile/).

Ismeretlen projektben kezdd ezzel: `bash ~/.claude/skills/callgraph/callgraph.sh profile [--project alias]`. A mappa-
és fájlnevekből (és a `composer.json` / `package.json` / `Cargo.toml` frameworkjéből) megmondja az architektúrát
(Laravel / Yii rétegzett MVC, DDD / hexagonális, Vue / React frontend, SwiftUI MVVM, Rust web), a rétegeket a
mappáikkal és a fájlszámukkal, a tervezési mintákat (Repository, Factory, Strategy, Observer…), az egyértelmű szerep
nélküli mappákat, és hogy a rétegek hogyan hívják egymást. A kód-gráf megerősíti: `extends Controller`,
`implements *RepositoryInterface`, `defineStore`, és a rétegek közti hívások iránya. Kb. 15 sor; a `--full` mappánként
részletez. Minden fájl kap egy szerepet bizonyossággal (magas / közepes / alacsony), vagy „ismeretlen” marad — nem
találgat. MCP: a `project_profile` eszköz, és kapcsolódáskor egy tömör profil is megy.

A rétegek fájlszám szerint sorakoznak, így a fő réteg mindig felül van. A backendben és a frontendben is meglévő
szerep külön látszik, pl. `controller (php)` és `controller (js)`, ha PHP-controllerek mellett AngularJS-controllerek
is vannak, és a hívásirány sem köti össze a kettőt. A projektbe bemásolt külső kód (saját `composer.json`,
`package.json` vagy `LICENSE` fájlos almappa, vagy `vendors/`, `third_party/`, `extensions/`, `libs/` jellegű mappa) és
a build-kimenet (`dist/`, `build/`, `*.min/`, minifikált fájlok) nem számít: egy sor mondja meg, hány fájl maradt ki és
honnan (a `profile --with-vendor` mégis beszámolja őket); a lekérdezések ezeket a fájlokat is látják. Az `event`, a
`listener` és a `job` réteghez bizonyíték kell a kódban (Event / Listener ősosztály, a Laravel `Dispatchable` vagy
`ShouldQueue`, `handle(…)` metódus a `Listeners` névtérben) — egy csak `Event` nevű mappa nem event-réteg.

A rétegek a lekérdezésekben (mind kapcsolóra, az alapkimenet nem változik):

```bash
bash ~/.claude/skills/callgraph/callgraph.sh app --layer service                     # csak a service-fájlok (név,név...)
bash ~/.claude/skills/callgraph/callgraph.sh . --layer controller:js                 # csak egy ökoszisztéma: php, js, swift, rust
bash ~/.claude/skills/callgraph/callgraph.sh app --format summary --layers           # summary rétegenként
bash ~/.claude/skills/callgraph/callgraph.sh app --grep 'Repo::find' --depth 2 --by-layer   # hívók rétegenként
```

A `--by-layer` a hívott metódushoz legközelebbi réteget teszi előre (model → repository → service → controller), és
minden csoport egy `[service]  3 hívás` jellegű fejléccel kezdődik, a rövid és a hosszú `--grep` nézetben egyaránt
(`--depth`-tel minden szinten).

Semmitmondó (vagy nem angol) mappanevek? Egy commitolható `.callgraph-layers` fájl a projekt gyökerében
hozzárendeli őket: `service = app/Szolgaltatasok, app/Logic`, `ignore-role = app/Legacy`.

### Nyelv

A terminál-kimenet és a súgó alapból angol; magyarul is kérhető:

```bash
bash ~/.claude/skills/callgraph/callgraph.sh set-lang hu    # elmentés (vissza: set-lang en)
bash ~/.claude/skills/callgraph/callgraph.sh set-lang       # az aktuális nyelv és a forrása
CALLGRAPH_LANG=hu bash ~/.claude/skills/callgraph/callgraph.sh app/Models/Order.php   # csak egy futásra
```

### Más AI-eszközökkel (MCP)

Lépésről lépésre, kliensenként (Claude Code, Claude Desktop, Cursor, VS Code): [callgraph.eu/hu/mcp](https://callgraph.eu/hu/mcp/).

A Claude Code-skill mellett (változatlan, teljes értékűen támogatott) ugyanaz a bináris
[MCP](https://modelcontextprotocol.io)-szerver is stdio-n, így bármely MCP-kliens használhatja: `callgraph mcp`.
Eszközök: `locate`, `outline`, `method_body`, `find_callers`, `search`, `framework_view`, `notes`, `projects`, `guide`, `project_profile`. Ugyanazt
a tömör szöveget adják, mint a CLI, ugyanazzal a kimeneti korláttal; csak olvas, az utak a projekt-gyökéren belül
maradnak, a lekérdezések nem használnak hálózatot (csak a lenti, naponta egyszeri frissítés-ellenőrzés). Kapcsolódáskor a szerver rövid használati útmutatót küld a kliensnek a beállított
nyelven (melyik eszköz mire, hogyan keresd a hívókat, hogyan olvasd a kimenetet); a Claude Code ezt az agent
utasításai közé teszi. A `guide` eszköz a teljes skill-útmutatót adja. Mindkettő a `SKILL.md` forrásából készül, így
mindig egyezik vele.

A telepítő a Claude Code-ban regisztrálni is tudja: a `bash install.sh` rákérdez (ha a `claude` parancs elérhető), a
`bash install.sh --mcp` kérdés nélkül megcsinálja, a `--no-mcp` kihagyja. Eltávolítás:
`claude mcp remove -s user callgraph`. Kézzel:

```bash
# Claude Code, macOS / Linux (a géphez illő binárist választja):
claude mcp add -s user callgraph -- \
  ~/.claude/skills/callgraph/bin/$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)/callgraph mcp --auto-update
```

```powershell
# Claude Code, Windows (PowerShell):
claude mcp add -s user callgraph -- "$env:USERPROFILE\.claude\skills\callgraph\bin\windows-x86_64\callgraph.exe" mcp --auto-update
```

Claude Desktop, Cursor és VS Code: JSON-bejegyzés `"command": "<a bináris abszolút útja>", "args": ["mcp", "--auto-update"]` alakban.
**Ajánlott: `--auto-update`** — indításkor (naponta legfeljebb egyszer) a szerver pontosan úgy telepíti az újabb
kiadást, mint a `callgraph.sh update` (SHA-256-ellenőrzéssel, a beállításaid megmaradnak), és újraindul, így a kliens
mindig a legújabb verzióval beszél; ha bármi nem sikerül, egyszerűen a jelenlegivel indul. A kapcsoló nélkül csak egy
egysoros „újabb verzió érhető el” megjegyzést tesz a munkamenet első válasza végére. Csak a callgraph saját mappájába
ír; minden frissítés-ellenőrzés kikapcsolható: `CALLGRAPH_UPDATE_CHECK=0`.
Receptek, paraméterek, biztonság: [`docs/MCP.md`](docs/MCP.md).
Futás közben az MCP-szerver figyeli a betöltött projekteket, és a változott fájlokat magától frissíti.

### A projekt-térkép frissen tartása

Egy `git pull` vagy sok szerkesztés után a `bash ~/.claude/skills/callgraph/callgraph.sh refresh [--project alias]`
(`--background`-dal azonnal visszatér) előre frissíti a projekt-térképet, így a következő lekérdezésnek nem kell
újraelemeznie. Opcionális Claude Code-hook: a `bash install.sh --hooks` (vagy `callgraph.sh hook install`) minden
Claude által szerkesztett fájlt a háttérben frissít; egyetlen bejegyzést vesz fel a `~/.claude/settings.json`-ba, a
többit megtartja, és előtte `settings.json.bak` mentést ír. Eltávolítás:
`bash ~/.claude/skills/callgraph/callgraph.sh hook uninstall`. Részletek: [`docs/PROJECT-GRAPH.md`](docs/PROJECT-GRAPH.md).

### Helyi használati statisztika

A callgraph a gépeden egy kis használati összesítőt vezet: melyik parancs vagy MCP-eszköz hányszor futott, volt-e
hiba, és sávokban mennyi ideig tartott és mekkora volt a kimenete. Az MCP-n futó `locate`-nél azt is, hogy az agent
utána az első találatot nyitotta-e meg. Utat, fájl- és szimbólumnevet, keresési kifejezést, projektnevet és
hibaszöveget soha nem rögzít, és **semmit nem küld el sehova** (hálózatot nem használ). Megtekintés:
`bash ~/.claude/skills/callgraph/callgraph.sh stats [--days N]` (alapból az elmúlt 7 nap). Az adat helye:
`~/.claude/skills/callgraph/stats/` (havonta egy kis fájl, kb. 90 napig marad meg); kikapcsolás:
`CALLGRAPH_STATS=0`. CI-ben és skill-telepítésen kívül magától ki van kapcsolva.

### Visszajelzés az agentektől

Minden hibaüzenet kap egy `[error-id: …]` azonosítót. Az agent (vagy te) megjegyzést fűzhet hozzá, így a rossz
esetek könnyen összegyűjthetők és javíthatók:

```bash
bash ~/.claude/skills/callgraph/feedback.sh add <error-id> "mit vártam, mi történt"
bash ~/.claude/skills/callgraph/feedback.sh list --open
```

## Hogyan működik

```
fájlok ─► FileWalker ─► LanguageRegistry ─► Language::analyze ─► FileGraph ─► JSONL cache / projektindex ─► lekérdező formátumok
          (.gitignore,    (kiterjesztés      (mago · oxc · raffia ·   (egy közös
           vendor, …)      szerint)           pulldown-cmark · syn ·    séma)
                                              tree-sitter-swift)
```

- Minden nyelv egy modul a `src/lang/<nyelv>/` alatt, ami az AST-jét egy közös `FileGraph` sémára képezi le; a
  lekérdező réteg (`src/query/`) nem tudja, melyik nyelvből jött egy fájl.
- Parserek: [mago](https://github.com/carthage-software/mago) (PHP), [oxc](https://github.com/oxc-project/oxc)
  (JS / TS / Vue), [raffia](https://github.com/g-plane/raffia) (CSS / SCSS),
  [pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark) (Markdown), [syn](https://github.com/dtolnay/syn) (Rust), [tree-sitter-swift](https://github.com/alex-pinkus/tree-sitter-swift) (Swift).
- Az eredmények fájlonként JSONL-be cache-elődnek (mtime-alapon); a „ki hívja?” egy inkrementális projektindexet
  (mtime + méret) használ a `.callgraph/` alatt.

Felépítés és kódolási szabályok: [`ARCHITECTURE.md`](ARCHITECTURE.md) · lekérdező réteg: [`docs/QUERY.md`](docs/QUERY.md) ·
nyelvenkénti leképezés: [`docs/LANG-JS.md`](docs/LANG-JS.md), [`docs/LANG-CSS.md`](docs/LANG-CSS.md),
[`docs/LANG-DOC.md`](docs/LANG-DOC.md), [`docs/LANG-RUST.md`](docs/LANG-RUST.md), [`docs/LANG-SWIFT.md`](docs/LANG-SWIFT.md).

## Fejlesztés

Helyben csak Docker kell; a toolchain (Rust stable, rustfmt, clippy, musl target, python3, php) a fejlesztői
image-ben van (`docker/dev.Dockerfile`):

```bash
bash tools/dev.sh check     # fmt --check + clippy -D warnings + tesztek (debug profil, mint a CI) — minden commit előtt
bash tools/dev.sh build     # release bináris: ~/.cache/callgraph-dev/target/release/callgraph
bash tools/dev.sh test      # csak a tesztek
bash tools/dev.sh skill     # dist/callgraph-<verzió>.tar.gz (a skill-csomag, Linux)
bash tools/dev.sh shell     # shell a konténerben
```

A buildkimenet minden munkapéldánynak (worktree-nek) közös (`CALLGRAPH_DEV_TARGET`, alapból
`~/.cache/callgraph-dev/target`; `CALLGRAPH_DEV_TARGET=local`: munkapéldányonkénti `target/docker/`), így egy új ág nem
fordít újra minden függőséget. A `check-release` ugyanezt a release-profillal futtatja (LTO, lassú).
Helyi toolchainnel a szokásos `cargo build --release` / `cargo test` is működik. A CI
([`.github/workflows/release.yml`](.github/workflows/release.yml)) minden pushnál buildel és tesztel macOS-re
(arm64, x86_64), Linuxra (arm64, x86_64, statikus musl) és Windowsra, `v*` tagnél pedig GitHub Release-t készít.

### Hibajelentés és visszajelzés

Hibát találtál, vagy ötleted van? [Nyiss egy issue-t](https://github.com/GeRiY/callgraph/issues/new/choose) — a hibajelentő űrlap kéri a `callgraph --version` kimenetét, a parancsot és a kimenetet (az `[error-id: …]` sorral).

### Közreműködés

Issue-kat és pull requesteket szívesen fogadunk. PR előtt futtasd a `bash tools/dev.sh check`-et.
Új nyelv = egy új modul + egy regisztrációs sor: lásd [`docs/UJ-NYELV.md`](docs/UJ-NYELV.md).
Refaktornál a kimenetnek bájtra azonosnak kell maradnia (`tools/identity_check.sh`).

### Biztonság

A sérülékenységeket privátban jelezd, a
[GitHub Security Advisories](https://github.com/GeRiY/callgraph/security/advisories/new) oldalon, ne nyilvános issue-ban — lásd [`SECURITY.md`](SECURITY.md).

## Szerző

Készítette és karbantartja: **Makra Gergő** — [LinkedIn](https://www.linkedin.com/in/gergomakra/).

## Licenc

[MIT](LICENSE)
