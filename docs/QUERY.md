# Query-réteg

A `show` parancs a `SelectorSet` segítségével fájlokat és metódusokat választ ki. A `QueryEngine` a projekt gyökeréhez relatív utat old fel, előbb a projekt `.callgraph/cache.jsonl` cache-ét használja, majd az `Analyzer` facade-et. A kimenetet a `FormatterRegistry` név szerint választott Strategy-je állítja elő.

Új formázó: hozz létre egy külön `Formatter` implementációt a `src/query/format/` alatt, majd adj hozzá egy `register`/névágatot a registryhez. A limit előtti formázás és a `method-index` változatlan marad.

## Felépítés
- `src/query/selector.rs` — `<fájl>[::m1[::m2]]` argumentumok (`SelectorSet`), a callgraph hibáival (szelektoros és sima argumentum keverése).
- `src/query/engine.rs` — `QueryEngine` (Facade): a gyökérhez relatív utak → `QueryResult` (FileGraph + kijelölt metódusok); cache: `<gyökér>/.callgraph/`, a cache-írás hibája nem állítja meg.
- `src/query/limit.rs` — `OutputLimit`: 400 sor (`CALLGRAPH_OUT_LIMIT`, `--full` kikapcsolja); mappán summary, fájlon `method-index`.
- `src/query/format/` — egy formázó = egy fájl, `FormatterRegistry::with_defaults()` regisztrálja.
  Paraméteres nézetek (`lines` sávval, `strings`/`const` mintával) a `FormatterRegistry::format_with`-en át.

## Nézetek és kapcsolók (`callgraph show`)
| Kapcsoló | Formázó | Megjegyzés |
|---|---|---|
| (alap) / `--format outline` | `outline.rs` | |
| `--format json` / `jsonl` | `json.rs` | a json a callgraph `jq -s '.'` paritása, a jsonl natív |
| `--body` | `body.rs` | a kijelölt metódusok forrása sorszámozva, docblockkal |
| `--lines n-m` | `lines.rs` (+`source.rs`) | nyers sorok, elemzés nélkül (legacy view-n is) |
| `--strings <minta>` / `--const <minta>` | `strings.rs` / `constants.rs` | |

## Nyelv
A kimenet emberi szövegei (fejlécek, címkék, „nincs találat”, figyelmeztetések, hibaüzenetek) a nyelvbeállítást követik
(`CALLGRAPH_LANG` › `callgraph lang` › angol); az adatok (azonosítók, kategóriák, JSON-kulcsok, utak) nem fordulnak.
A formázók a nyelvet paraméterként kapják (`Formatter::format(rows, lang)`, `FormatterRegistry::in_lang`, `OutputLimit::with_lang`).

## Mérés
`bash tools/query_check.sh "$PWD/target/release/callgraph show"` (a leképezés: `golden/query/rpg_map.tsv`, felülírható: `CALLGRAPH_MAP=<fájl>`).
A szkript `CALLGRAPH_LANG=hu`-val futtat: a golden a régi, magyar skill kimenete.

### R6b: `api`, `stats`, `summary`
| `--format api` / `stats` / `summary` | `api.rs` / `stats.rs` / `summary.rs` | a summary a mappás kimenet-korlát tartaléka is (400 sor fölött a figyelmeztető sorral) |

### R6e: `--grep`, `--category`, `--depth`, `--guess`
| `--grep <minta>` / `--category <k,...>` | `src/query/filter/` (Specification) + `QueryEngine::query_filtered` | 400 sor fölött találatonként 1 sor (`⚠ A --grep outline N sor lenne …`) |
| `--depth N` / `--guess` | `QueryEngine::grep_text` + `index::CallerIndex` | a csak név szerinti egyezést „?” jelöli; `--guess` nélkül a szint végén „(+N feloldatlan …)” |
Ismert maradék (99%+ FileGraph-eltérés, halasztva): a grep-outline sorszáma és néhány feloldatlan hívó a calls-mezőből.
Hibaüzenetek: az `[error-id]` sort a callgraph-burok (R6g) teszi hozzá — az callgraph csak az üzenetsort írja.

### R6c: `flow`, `hotspots`, `coverage`, `mermaid`, `dot`
| `--format flow` | `flow.rs` | elágazásonként az ágak és az ágban lévő hívások kategóriánként |
| `--format hotspots` | `hotspots.rs` | be-/kifok + ciklomatikus szám, top 20 (csak a saját metódusok) |
| `--format coverage` | `coverage.rs` | publikus metódusok docblock-lefedettsége |
| `--format mermaid` / `dot` | `graph_text.rs` | öröklési és hívási élek, bájtsorrendben, ismétlés nélkül |
A `::metódus` kijelölés szűrése közös: `selection.rs` (a callgraph jq-szűrőjének mása). A kimenet-korlát az outline, flow, api, mermaid és dot formátumra él
(mappán summary, egy fájlon metódus-index a tényleges formátumnévvel).

### R6f: framework-nézetek (`events`, `routes`, `middleware`, `validation`, `jobs`, `providers`, `magic`, `di`, `views`)
`src/query/format/framework/`: a közös `RuleView` formázó + nézetenként egy gyűjtőfüggvény (`rules.rs`), a regexek egyszer fordítva (`re!`, `LazyLock`).
Új nézet: egy gyűjtőfüggvény a `rules.rs`-ben + egy sor a `framework::register` táblájában.
**Szándékos eltérés:** a callgraph `jobs` mintájában a `later\\\\(` bezáratlan zárójelet ad, a jq hibával kilép, és a nézet mindig „(nincs job/queue találat)”;
az callgraph a szándékolt `later\(` mintát használja (így a `sp_jobs` golden-eset ELTÉR — a golden itt a hibát rögzíti).

## R6g: a `callgraph.sh` vékony burok (`skill/callgraph.sh`)
- A burok dolga: `--project <alias>` feloldása a `projects.json`-ból (`CALLGRAPH_REGISTRY`), a `KULCS=érték` elöl álló argumentumok exportja,
  a hibák `[error-id]` sora (`skill/lib.sh` `fail`), és a JS / TS / Vue cél továbbítása a régi motornak (`CALLGRAPH_LEGACY`) változatlan argumentumokkal.
- Az callgraph csak abszolút gyökeret (`--project <út>`) és címkét (`--label <alias>`) kap; projektnevet maga nem old fel.
- A bináris: `CALLGRAPH_BIN`, vagy `bin/<os>-<arch>/callgraph`, vagy `bin/callgraph`.
- A `show` a projekt `.callgraph/` tárolóját az `ensure()`-rel hozza létre (`*`-os `.gitignore`), olvas és ír is a cache-be; ha a tároló nem írható, cache nélkül fut.
- Mérés a burkon át: `CALLGRAPH_MAP=golden/query/cases.tsv bash tools/query_check.sh "CALLGRAPH_BIN=… PHP_AST_HOME=… CALLGRAPH_REGISTRY=… CALLGRAPH_LEGACY=… bash skill/callgraph.sh"` → 39/51 AZONOS
  (a hibaesetek is); a maradék a FileGraph 99%+ eltérése (G1d) és a szándékos `jobs`-javítás.
- Az élő `~/.claude/skills/callgraph/callgraph.sh` NINCS lecserélve — az átállás a felhasználó döntése.
