# callgraph — architektúra és kódolási szabályok (többnyelvű szerkezet)

Forráskód-elemző, ami fájlonként egy **JSONL** sort (egy JSON-objektumot) állít elő. A rendszer nem PHP-hez kötött:
**nyelv-modulokból** áll, a közös rétegek (modell, cache, bejáró, CLI) nyelvfüggetlenek. A PHP kimenete tartalmilag
azonos a `callgraph` skill `ast_graph.php`-jáéval (mérce: `golden/`). Új nyelv hozzáadása: `docs/UJ-NYELV.md`.

## Alapelvek (felhasználói követelmény, 2026-11-04)

1. **Külön fájlokból épül fel.** Egy fájl = egy felelősség. ~300 sor fölött a fájlt szét kell bontani
   (az adatfájl kivétel). A `main.rs` csak a CLI-belépő.
2. **Objektumorientált (Rust-módra).** Állapot + viselkedés `struct`-ban; a kiterjesztési pontok `trait`-ek; a
   függőségek a konstruktoron át jönnek (DI, generikus vagy `Box<dyn Trait>`). Öröklés helyett kompozíció.
   Publikus felület minimális (`pub(crate)` alapból).
3. **Egy új nyelv = egy új `src/lang/<nyelv>/` modul + egy regisztrációs sor.** A közös rétegekhez nem kell nyúlni.

## Szerkezet

```
src/
  main.rs, lib.rs, error.rs
  core/                         nyelvfüggetlen építőkövek
    chain.rs                    Chain<I> + Rule<I> — generikus Chain of Responsibility
    source.rs                   SourceFile { path, text, mtime } — a nyelvek bemenete
    recovery.rs                 végzetes szintaktikai hibánál az ép előtag vágási pontjai (JS/TS/Vue, Rust)
  lang/                         a nyelv-réteg
    language.rs                 trait Language (id, role, extensions, analyze), LanguageRole, Analysis
    backend.rs                  trait LanguageBackend: Language   — szerveroldal (framework-felismerés)
    frontend.rs                 trait LanguageFrontend: Language  — kliensoldal (modul-/alias-feloldás)
    document.rs                 trait LanguageDocument: Language  — dokumentum/adat (hivatkozás-feloldás)
    registry.rs                 LanguageRegistry (Factory): kiterjesztés → nyelv; a bejáró kiterjesztés-listája
    php/                        PhpBackend: impl LanguageBackend
      backend.rs                a csővezeték: MagoAdapter + visitorok + FileGraphBuilder
      parser/                   Adapter a mago-syntax köré
      analysis/                 visitorok, name_resolver, type_tracker, docblock, context
      categorize/               CallInfo + a 12 szabály + natív lista (a core::chain-re építve)
      framework/                stratégiák (laravel, yii, generic) + detektor
  model/                        közös kimeneti séma (FileGraph + FileGraphBuilder)
  analysis/                     Analyzer (Facade): fájl → registry.for_path → nyelv.analyze → Analysis
  io/                           walker (a registry kiterjesztéseivel), cache (CacheStore + JsonlCacheStore), jsonl_writer
  index/                        CallerIndex (fordított hívó-index, mindkét irány) + TypeHierarchy (via-illesztés)
  graph/                        ProjectGraph: perzisztens projekt-gráf (.callgraph/: index.bin manifest, graph.pack,
                                facts.bin), fájlonként frissítve; folyamaton belül gyökerenként egy példány;
                                előre-frissítés (refresh.rs: `callgraph refresh`, refresh.lock) és MCP-fájlfigyelő (watch.rs)
  hook/                         Claude Code PostToolUse hook: a hook JSON-ja → háttér-`refresh`; settings.json szerkesztése
  profile/                      projekt-profil: szerep-/réteg-, architektúra- és minta-felismerés a mappa- és fájlnevekből
                                (szabály-táblák adatként), megerősítés a gráfból; a `--layer` / `--layers` / `--by-layer`
                                kapcsolók osztályozója; `.callgraph-layers` (saját elnevezések)
  query/                        QueryEngine (Facade), formázók (Strategy + FormatterRegistry), kimeneti korlát
  locate/                       kiindulópont-kereső (Facade: Locator): kifejezések bontása (terms), egyezés a szűkített
                                tényeken (matcher), név szerinti hívószám (callers), rangsor (rank), indoklás + kimenet
  stats/                        helyi, passzív használati mérés (hálózat nincs): CLI-próba (main.rs) és MCP-próba
                                (mcp::server) → napi összesítő `<skill>/stats/YYYY-MM.json` (csak zárt nevek és
                                sávok, út / név soha), a `locate`-találat követése memóriában, `callgraph stats` riport
  cli/                          Command minta; args, parancsok (analyze, prune)
  mcp/                          MCP-szerver (`callgraph mcp`, rmcp, stdio): eszköz-paraméter → `show`-lekérdezés, útvonal-őr, frissítés (`--auto-update`, napi értesítés)
tests/                          integrációs tesztek (pl. languages.rs: teszt-nyelvek a registryn)
docs/                           ARCHITECTURE-uj.md, UJ-NYELV.md
```

Adatfolyam: `FileWalker` (a `registry.extensions()` szerint) → fájlok → `Analyzer::analyze_path` →
`registry.for_path(path)` → `Language::analyze(&SourceFile)` → `Analysis { graph, warnings }` → JSONL-író / cache.

## A három szerep

| Szerep | Trait | Nyelvek | Specifikus metódus |
|---|---|---|---|
| `LanguageRole::Backend` | `LanguageBackend` | PHP (kész), később Python, Go | `detect_framework(dir)` |
| `LanguageRole::Frontend` | `LanguageFrontend` | később CSS/SCSS, JS/TS/Vue | `resolve_module(spec, from)` |
| `LanguageRole::Document` | `LanguageDocument` | később JSON, Markdown, YAML, TOML | `resolve_reference(reference, from)` |

Mindhárom a közös `Language` traitre épül (`id`, `role`, `extensions`, `analyze`), és ugyanabba a `FileGraph` sémába
képez le. A registry a regisztráció sorrendjében keres: előbb a backendek, utána a frontendek, végül a dokumentum-nyelvek.
A kiterjesztés-egyezés kis-/nagybetű érzékeny, a kiterjesztések pont nélküliek, kisbetűsek. Ha két nyelv ugyanazt a
kiterjesztést kéri, az előbb regisztrált nyer.

## Tervezési minták

| Minta | Hol | Mit old meg |
|---|---|---|
| **Abstract Factory** | egy nyelv-modul (pl. `lang::php`) | a nyelv a saját parserét, visitorait, szabályait, stratégiáit adja együtt |
| **Factory** | `lang::LanguageRegistry` | kiterjesztés → a megfelelő nyelv; a bejáró kiterjesztés-listája |
| **Facade** | `analysis::Analyzer` | egy belépő: fájl → `Analysis`; a CLI csak ezt látja |
| **Adapter** | `lang::php::parser::PhpParser` + `MagoAdapter` | a külső parser-crate (mago-syntax) elrejtése, cserélhető |
| **Visitor** | `lang::php::analysis::visitor::*` | az AST bejárása szempontonként (struktúra, hívások, vezérlés, stringek, kommentek, írások) |
| **Builder** | `model::FileGraphBuilder` | a visitorok ebbe gyűjtenek; a végén immutábilis `FileGraph` |
| **Chain of Responsibility** | `core::Chain` + `lang::php::categorize::rules::*` | hívás-kategória: szabályok sorban, az első illeszkedő dönt |
| **Strategy** | `lang::php::framework::FrameworkStrategy` (`laravel`, `yii`, `generic`) | framework-specifikus ismert nevek / kategóriák |
| **Factory** | `lang::php::framework::FrameworkDetector` | composer.json alapján a megfelelő stratégia (könyvtáranként cache-elve) |
| **Repository** | `io::CacheStore` trait + `JsonlCacheStore` | mtime-alapú cache JSONL-ben, cserélhető tároló |
| **Repository** | `graph::ProjectGraph` | a projekt fájljainak gráfja egy hozzáfűzős packben + manifest + szűkített tények, mtime + méret szerint frissítve |
| **Command** | `cli::commands::*` | CLI-alparancsok egységes felülettel |

## Közös rétegek

- **`model/`** — a kimeneti séma (serde, a JSON 1:1). Minden nyelv ide képez le; a nem-kód nyelvek leképezése:
  CSS/SCSS — container: a stíluslap, „metódus”: mixin / function / szabály-blokk, imports: `@use`/`@import`;
  JSON — constants: kulcs-útvonalak; Markdown — „metódus”: címsor-szakaszok, „hívás”: linkek.
- **`io/`** — `FileWalker::for_registry` a regisztrált kiterjesztéseket járja be (a `vendor`, `node_modules`, `.git`… mappákat kihagyja);
  a cache kulcsa az abszolút út, értéke mtime + az eszköz verziója.
- **`graph/`** — a perzisztens projekt-gráf (`docs/PROJECT-GRAPH.md`): egy hozzáfűzős pack (`graph.pack`: fájlonként
  a frontend `FileGraph`-ja veszteségmentes tükör-alakban + a hívás-sorok; MessagePack + LZ4), egy kis manifest
  (`index.bin`: út → bélyeg + a rekord helye) és a szűkített tények (`facts.bin`); minden fájl fejléce: fajta, verzió,
  build-ujjlenyomat (eltérésre csendes újraépítés). Indításkor csak a manifest töltődik; a teljes hatókörű
  lekérdezések (`--grep`, `--depth`, `callers`, `--strings`, `--const`) a `facts.bin`-ből, az egyfájlos nézetek a pack
  rekordjaiból válaszolnak — egy meleg lekérdezés a projekt méretétől függetlenül állandó számú fájlt nyit. Minden
  `show` és a `callers` ebből olvas — a pack a fájlonkénti cache is; a meleg lekérdezés forrásfájlt nem elemez. A levél-típusok új mezői kódváltozás nélkül tárolódnak; egy új `FileGraph`- /
  `Property`- / `Constant`-mező a `graph/codec.rs` tükrében fordítási hibát ad (egy sor).
- **`analysis/`** — az `Analyzer` nem ismer nyelvet: ismeretlen kiterjesztésre `CallgraphError::Unsupported`.
- **`core/`** — a nyelvek által közösen használt, nyelvfüggetlen típusok.

## Szabályok a kódhoz

- `cargo fmt` + `cargo clippy --release -- -D warnings` tisztán; edition 2024.
- Minden publikus típusnak/traitnek rövid doc-kommentje van (miért létezik, nem regény).
- **Formátum: JSONL mindenhol, JSON-fájl nem.** Kimenet, cache és golden referencia is soronként egy JSON-objektum;
  konfigurációt (`composer.json`, `jsconfig.json`) természetesen JSON-ként olvasunk. Kivétel: a skill-mappa kis,
  egyetlen objektumból álló állapotfájljai (`projects.json`, `update-check.json`) JSON-ok — ezek nem rekordlisták.
- A mezőnevek és -sorrend a `golden/ref.jsonl` szerint.
- **Elfogadás refaktornál:** bájtra azonos kimenet az alapvonallal (`tools/identity_check.sh`), a golden egyik mezőben sem romlik.
- Parser: `mago-syntax = "=1.59.0"` (pontos verzió); mago-specifikus típus csak `lang/php/parser/` és
  `lang/php/analysis/visitor/` alatt jelenhet meg. Részletek: `spike/DONTES.md`.
- Release profil: `lto = true`, `codegen-units = 1`, `strip = true`, `opt-level = 3`.
- Git-parancsot az agentek nem futtatnak.
