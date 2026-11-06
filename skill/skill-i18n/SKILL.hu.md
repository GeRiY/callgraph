---
name: callgraph
description: PHP / Rust / JS / TS / Vue / Swift kódbázis token-hatékony feltérképezése AST-elemzéssel, a fájl beolvasása NÉLKÜL — kategorizált hívás-gráf (osztály/komponens, metódus/függvény sortartománnyal), metódus-törzs, "hol hívják" keresés, hatáselemzés (--depth), string- és konstans-keresés, vezérlési ágak, framework-nézetek (routes, validation, events…). Használd PHP-, .rs-, .vue-, .js-, .ts-, .swift-fájl megértéséhez, hívási kapcsolatok, "ki hívja / hol használják" kérdésekhez — ez legyen az ELSŐ lépés a teljes Read/Grep helyett. NE használd nem kód fájlra (md, json, css, sima szöveg), vagy ha a pontos fájlszöveg kell egy már ismert sávban (akkor --lines vagy célzott Read).
---

# callgraph — kód-térkép Read nélkül

`bash <skill>/callgraph.sh …` (a `<skill>` ennek a skillnek a mappája, pl. `~/.claude/skills/callgraph`).
Minden célra (PHP, Rust, JS / TS / Vue, Swift / SwiftUI, CSS / SCSS, JSON, MD) a natív `callgraph` bináris (`callgraph show`) dolgozik; a kimenet közös
szerkezetű: container (osztály / komponens / modul) → metódusok sortartománnyal → hívások kategóriánként.

## 0. lépés — projekt regisztrálása (egyszer, projektenként)

```bash
bash <skill>/projects.sh add myapp /abszolút/út/a/projekt/gyökeréhez
bash <skill>/projects.sh list
```

Utána `--project myapp` mellett a gyökérhez képesti rövid út adható meg. `--project` nélkül az aktuális
mappa a gyökér.

## Gyors parancsok

```bash
CG=<skill>/callgraph.sh
bash $CG --project myapp app/Http/Controllers/X.php                  # outline: osztály, metódusok sortartománnyal, hívások kategóriánként
bash $CG --project myapp app/Http/Controllers/X.php::store           # csak egy metódus (több ::szelektor, több fájl is)
bash $CG --project myapp app/Http/Controllers/X.php::store --body    # a metódus forrása, tömören (kommentekkel, docblock nélkül: --doc; --raw = pontos)
bash $CG --project myapp app/Http/Controllers/X.php --lines 30-60    # tetszőleges sáv sorszámozva (view, procedurális fájl)
bash $CG --project myapp app --strings 'Order not found'             # string-literál (hibaüzenet, kulcs) helye
bash $CG --project myapp app --const STATUS_ACTIVE                   # konstans: definíció, hivatkozások, továbbexportálás
bash $CG --project myapp app --grep '(^|::|\.|->)cancel$'           # ki hívja: a név MINDEN hívása, a hívás sorával
bash $CG --project myapp app --grep 'Order::cancel'                  # csak az Order-re feloldott hívások (+ interfészen / szülőn / trait-en át)
bash $CG --project myapp app --grep 'Order::cancel' --depth 3        # hatáselemzés: a hívók hívói
bash $CG --project myapp app --grep 'Order::cancel' --depth 3 --guess # + a csak név szerinti egyezések („?”)
bash $CG --project myapp app --grep 'Order::cancel' --body           # a talált hívó metódusok forrása
bash $CG --project myapp app --category model_use,orm_relation       # szűrés hívás-kategóriára
```

**Hívók keresése:** a név szerinti mintával kezdd: `'(^|::|\.|->)név$'`. Minden hívást megtalál, akkor is, ha a fogadó
típusa ismeretlen, örökölt vagy trait-ből jön. A `Típus::metódus` csak az erre a típusra feloldott hívásokat adja (és
az interfészén / szülőjén / trait-jén át menőket), ezért hívók maradhatnak ki; gyakori, zajos névnél (`save`, `find`,
`get`) szűkítésre használd.

**A keresőkimenet olvasása** (`--grep`, `--depth`, `--strings`, `--const`, mappák):
- Első sor `gyökér: /abs/út`: utána minden út ehhez a projektgyökérhez relatív; így add vissza őket (ugyanazzal a
  `--project`-tel).
- `L25 │ $this->repo->find($id)`: a hívás saját forrássora. Csak azért, hogy lásd, hogyan hívják, nem kell `--body`;
  a teljes logikához igen.
- `via Resolver`: a hívás a keresett típus interfészén, szülőosztályán vagy trait-jén át megy.
- `L12, L40 │ …`: ugyanaz a hívás több soron; hívónként legfeljebb 3 sor, utána `… +2 további: L55, L61`.
- `?` / `?::név`: ismeretlen fogadó-típus, csak név szerinti egyezés; lehet téves, nézd meg a sort.

## Melyik `--format` mire való

| Formátum | Mit ad |
|---|---|
| `outline` (alap) | szerkezet + hívások kategóriánként |
| `api` | publikus szerződés + docblock, implementáció nélkül |
| `flow` | if/match/switch ágak sortartománnyal (feltétel → mit hív); `::metódus`-sal szűkíthető |
| `summary` | mappa: fájlonként 1 sor |
| `hotspots` | top 20 metódus be-/kifok és ciklomatikus szám szerint |
| `coverage` | dokumentálatlan publikus metódusok |
| `stats` | kategória-statisztika |
| `mermaid` / `dot` | vizualizációhoz (öröklési és hívási élek) |
| `json` / `jsonl` | nyers gráf (nem korlátos); a `jsonl` az callgraph natív kimenete |

**Framework-nézetek** (`--format …`): `routes`, `middleware`, `validation`, `jobs`, `events`, `providers`,
`magic`, `di`, `views`. Mappára futtasd (pl. `bash $CG --project myapp routes --format routes`).

## JS / TS / Vue

Ugyanazok a kapcsolók. A „metódusok”: `setup` (a `<script setup>` top-level kódja), `template` (komponensek,
eseménykezelők, `$t()` / `route()`), függvények, `computed`, `watch@L<n>` / `onMounted@L<n>` hookok,
composable belső függvényei (`useX.belso`), Options API `methods` / `computed.x`. TS: a típusos paraméteren /
property-n (`repo: Repo`, `constructor(private repo: Repo)`, `new Repo()`) át hívott metódus `Repo::find` lesz, így a
`Típus::metódus` keresés és a `via <Interfész>` TS-ben is működik; típus nélküli, `any` vagy unió fogadónál név szerint
keress. Props = properties,
emits = `emit:<név>`. Kereshető kulcsok a `--grep`-hez:

```bash
bash $CG --project myapp resources/js --grep 'useForm.js::submit'     # <fájl>::<export>, import-feloldással, @/ aliasszal
bash $CG --project myapp resources/js --grep 'route::orders.show'     # route-használat
bash $CG --project myapp resources/js --grep 'i18n::orders.title'     # fordítási kulcs helye
bash $CG --project myapp resources/js --grep 'vue::emit:save'         # ki emitál 'save'-et
bash $CG --project myapp resources/js --grep 'Atoms/AtomButton.vue'   # hol használják a komponenst
bash $CG --project myapp resources/js --grep 'api.fetch'             # objektum-literál metódusa (const api = { fetch() {} })
```

## Rust

Container = a fájl-modul (`module`), `struct` / `enum` / `trait` / inline `mod`; az `impl` blokkok metódusai a típushoz
kerülnek (`implements` = a traitek). Kategóriák: `internal_helper` (saját), `module_use` (a crate más modulja, `resolved`
= `modul::fn` / `Típus::fn`), `native_rust` (std), `framework_use` (axum, actix-web, tokio, serde…), `package_use`
(más crate), `macro_use` (makró; a törzse nincs kibontva), `external_call` (ismeretlen fogadó).

```bash
bash $CG --project myapp src/api/users.rs                            # outline: struct/impl/trait, fn-ek sortartománnyal
bash $CG --project myapp src --grep 'Repo::find'                      # ki hívja (crate-belső: Típus::fn / modul::fn)
bash $CG --project myapp src --const MAX_ITEMS                        # const / static (Típus::CONST is): definíció + használat
bash $CG --project myapp src/api/users.rs --format api               # pub elemek, az impl-blokkok pub fn-jei is
```

Az értéken hívott metódus (`x.load()`) `Típus::load`, ha az `x` típusa egyértelmű (paraméter, `let x = Típus::new()`,
`self`, saját struct mezője), különben `?::load`; a `Típus::load` keresés ezeket is listázza, bizonytalanként.

## Swift / SwiftUI

Ugyanazok a kapcsolók. Container: `class` / `struct` / `enum` / `protocol` / `actor` / `extension`, a `View`-nak
megfelelő típus `component`; a fájl-szintű függvények a `<Fájl>.swift` `module` containerben. A `var body: some View`
és a többi computed property metódus; a property wrapper (`@State`, `@Published`, `@EnvironmentObject`…) a
property típusa előtt látszik. Hívás-kategóriák: `swiftui_view`, `swiftui_modifier`, `component_use`,
`native_swift`, `internal_helper`, `external_call`. Részletek: `docs/LANG-SWIFT.md`.

## Kimenet-korlát

400 sor fölött a mappás outline `summary`-ra, az egyfájlos outline metódus-indexre, a `--grep` találatonként
1 sorra vált; a `--body` / `--lines` / `--strings` / `--depth` 400 sornál megáll. Szűkíts (`::név`, `--grep`,
almappa); a `--full` kikapcsolja a korlátot, csak indokolt esetben. A küszöb: `CALLGRAPH_OUT_LIMIT`. Elöl álló `KULCS=érték` argumentum környezeti változóként él tovább
(pl. `bash $CG CALLGRAPH_OUT_LIMIT=100 …`).

## Új verzió jelzése

Ha egy `callgraph` kimenet utolsó sora `ℹ callgraph X.Y.Z is available (installed: …) — update: …` vagy
`ℹ callgraph X.Y.Z elérhető (telepítve: …) — frissítés: …`, szólj a felhasználónak (egyszer), hogy új verzió érhető el,
és ajánld fel a `bash <skill>/callgraph.sh update` futtatását. A felhasználó beleegyezése nélkül ne frissíts. A sor nem
a vizsgált kód része; az ellenőrzés a háttérben, naponta legfeljebb egyszer fut (kikapcsolás: `CALLGRAPH_UPDATE_CHECK=0`).

## Ha hibázik — visszajelzés

Minden hibaüzenet mellé `[error-id: xxxxxxxx]` jár, a GitHub hibajelentő űrlap linkjével. Ha az eredmény hibás,
hiányos vagy félrevezető, **javasold a felhasználónak, hogy jelezze GitHub issue-ban**
(https://github.com/GeRiY/callgraph/issues/new?template=bug_report.yml), és add meg neki, mit másoljon be: a parancsot, a kimenetet és a
`bash <skill>/feedback.sh show <error_id>` kimenetét (ez a helyi naplóbejegyzés). Issue-t a felhasználó nevében magadtól ne nyiss.

Helyi jegyzet (nem megy fel sehova, a gépen marad):

```bash
bash <skill>/feedback.sh show <error_id>                # a naplózott hiba részletei (ezt kell az issue-ba másolni)
bash <skill>/feedback.sh add <error_id> "<mit vártál, mi történt>"
bash <skill>/feedback.sh list --open
```

A kerülőút (célzott Read, Grep) ettől még használható, de a visszajelzés ne maradjon el.

## Jegyzetek (pilot)

Az outline-ban és a `--body`-nál egy fájl vagy metódus alatt `📝 <idő> · <szöveg>` sorok lehetnek: emberek vagy agentek
jegyzetei. Vedd figyelembe őket. Hagyj jegyzetet, ha tartós tanulságot találsz (miért van így, buktató) — röviden,
feladatlista nélkül. A `note list` az `[árva]` jegyzeteket is jelzi (a metódus már nincs meg; törlés: `note rm <id>`).

```bash
bash $CG --project myapp note add app/Order.php::cancel "A visszatérítés a RefundJob-ban fut, nem itt"
bash $CG --project myapp note list app
bash $CG --project myapp app --notes 'refund'
```

## Adat és cache

Minden adat a projekt saját `.callgraph/` mappájába kerül (cache, projekt-térkép `index.jsonl`, jegyzetek; csak a
változott fájlok frissülnek, így az ismételt keresés gyors; a mappa magától létrejön,
`*`-os `.gitignore`-ral, így a verziókezelésbe nem kerül). A projekten kívül semmi nem íródik. Ha a mappa
nem írható, cache nélkül fut. Takarítás: `callgraph prune` (lásd README).

## Korlát

Statikus elemzés: dinamikus hívás (`$this->$method()`, `obj[key]()`), magic method és futásidőben összeállított
osztálynév / regisztrált komponens nem oldható fel. A `--depth` alapból csak a biztos (feloldott) hívókat követi;
a név szerinti egyezést a `--guess` mutatja „?” jellel. A pontos vezérlési logikához a `--body` kell (tömör, kommentekkel; a `--raw` a pontos szöveget adja).
