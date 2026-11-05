# Projekt-gráf (`.callgraph/`)

Minden `show`-lekérdezés (outline, `--body`, `--grep`, `--depth`, `--strings`, `--const` …), a `callers` parancs és
az MCP-szerver ugyanezen eszközei egy **perzisztens projekt-gráfból** olvasnak, amely a projekt `.callgraph/`
tárolójában él és fájlonként frissül. A pack egyben az elemzési cache is (a régi `cache.jsonl` helyett).

Kód: `src/graph/` — `ProjectGraph` (`store.rs`), a bináris formátum (`codec.rs`), a fájlonkénti adatok
(`record.rs`), a folyamaton belüli példány (`pool.rs`).

## A tároló

```
.callgraph/
  index.bin          manifest: forrásfájlonként út, mtime (ns), méret, a teljes rekord helye a packben
  graph.pack         a teljes rekordok, csak hozzáfűzve: FileGraph (veszteségmentesen) + hívás-sorok
  facts.bin          a szűkített rekordok fájlonként: a teljes hatókörű lekérdezések mezői + hívás-sorok
  notes.jsonl        a jegyzetek (külön funkció, a gráf nem nyúl hozzá)
  .gitignore         `*`
```

A fájlok száma a projekt méretétől független: egy meleg lekérdezés legfeljebb a manifestet, a `facts.bin`-t és a
packet nyitja meg (`graph::tests::a_warm_query_opens_a_constant_number_of_files`).

- **`graph.pack`:** a rekordok `[u32 hossz][bájtok]` keretek egymás után; a manifest tartja minden fájl rekordjának
  eltolását és hosszát. Változott fájl: az új rekord a végére kerül (egyetlen hozzáfűzés az összes változott
  fájlnak), a régi holt terület lesz; ha a holt terület a pack felénél több (és a pack 1 MB fölött van), a pack az
  élő rekordokkal újraíródik. Hideg építés: egyetlen pack-írás. Minden rekord a saját útját és bélyegét is hordozza;
  ha nem egyezik a manifesttel (pl. egy másik folyamat közben tömörített), a fájl újraelemződik.
- **`facts.bin`:** fájlonként a `FileGraph` szűkített alakja (`FileFacts::slim`): névtér, konténerek, metódusok
  (doc és törzs nélkül), hívások (argumentumok nélkül), vezérlési ágak, string-literálok, konstansok, és a
  hívás-sorok. Ebből válaszol az 1. szintű `--grep` (a szűrő a property-ket, konstansokat és kommenteket úgyis
  eldobja), a `--strings`, a `--const`, és ebből épül memóriában a fordított hívó-index (`index::CallerIndex`), a
  típus-hierarchia (`index::TypeHierarchy`, a `via` illesztéshez) és a metódus-tartományok (`L<n>-<m>`) a `--depth`
  és a `callers` számára. Egyetlen olvasás, a keretek párhuzamos dekódolása.
- A teljes rekord (a pack) csak az egyfájlos nézetekhez kell (outline, `--body`, `--lines`, `api`, `flow`, `json` …),
  és ahhoz, hogy egy változott fájl szűkített rekordja elkészüljön. Több kért rekordnál a pack egyetlen olvasással
  jön be, kevésnél rekordonként pozícionált olvasással.

## Formátum

Minden fájl keretezett: `CGB1`, utána keretek: `[u32 LE hossz][bájtok]`. Az első keret a fejléc (MessagePack):

```
{ kind: "callgraph-manifest" | "callgraph-pack" | "callgraph-facts", version: 3, tool: "<io::tool_version()>" }
```

- `version`: a tárolt szerkezet verziója (`codec::FORMAT_VERSION`);
- `tool`: verzió + séma + a forrás ujjlenyomata — egy új build (bármelyik nyelvi frontend változása) a régi
  elemzéseket nem használja újra.

A többi keret MessagePack (mezőnevekkel), LZ4-gyel tömörítve (`rmp-serde`, `lz4_flex`; tiszta Rust). A manifestnek
egy kerete van (a bejegyzések listája), a packnek és a `facts.bin`-nek fájlonként egy (így párhuzamosan
dekódolható, és egy fájl változásakor a `facts.bin` többi kerete kódolás nélkül visszaíródik).

**Veszteségmentesség.** A `FileGraph` JSON-alakú serde-je szándékosan nem teljes (a kimenet a golden referenciát
követi: pl. `Property::js`, `Constant::caller` / `js` és az üres `Imports` objektum-jelzője kimarad). A tároló ezért
egy tükör-alakot használ (`codec::StoredGraph`, `StoredProperty`, `StoredConstant`), amely minden mezőt megőriz; a
levél-típusok (`Call`, `Method`, `Container`, `Comment`, `ControlFlow`, `StringLit`, `Write`) a saját serde-jükkel,
mezőnevekkel kerülnek bele — egy új mezőjük kódváltozás nélkül tárolódik. A `FileGraph`, `Property` és `Constant`
mezőit a tükör kimerítő szétbontással veszi át: egy új mező itt fordítási hibát ad (egy sor hozzáadása), nem csendes
adatvesztést. Teszt: `graph::tests::stored_graphs_round_trip_exactly` (minden golden minta + PHP / JS / Vue).

## Frissítés és érvénytelenítés

- Indításkor csak az `index.bin` töltődik be (kicsi). A frissesség mindig a bélyeg: mtime + méret (stat, a tartalmat
  nem olvassa).
- Egy lekérdezés a saját fájljaira nézi a bélyeget; a `--depth` 2. szintje és a `callers` a teljes projektre (stat
  minden fájlra — így egy érintetlen fájl új hívója sem marad ki). Csak a változott / új fájlok elemződnek
  (párhuzamosan), az új rekordjuk a pack végére kerül, és a `facts.bin`-ben csak az ő bemenetük cserélődik.
- A teljes projekt-listában nem szereplő (törölt, átnevezett) fájlok kikerülnek (a pack-rekordjuk holt terület).
- Hiányzó, sérült vagy más build által írt manifest → a tároló (pack, `facts.bin`) törlődik és csendben újraépül.
  Sérült pack-rekord → csak az a fájl elemződik újra. Sérült `facts.bin` (vagy keret) → a szűkített rekordok a
  packből állnak vissza.
- **Átállás:** a korábbi tárolók (`index.jsonl` — a JSONL gráf, `cache.jsonl` — a v0.2 fájlonkénti JSON-cache,
  `graph/` + `callers.bin` — a fájlonkénti shardos változat) az új tároló első építésekor törlődnek. (Az alacsony
  szintű `analyze` parancs a saját JSON-cache-ét továbbra is használhatja; a lekérdezések nem.)
- A manifest, a `facts.bin` és az új / tömörített pack írása atomikus (ideiglenes fájl, majd átnevezés); a
  pack-hozzáfűzés nem, de egy félbeszakadt hozzáfűzés csak holt terület (a manifest csak utána íródik).

## Folyamaton belül: egy példány gyökerenként

`graph::shared(gyökér)` gyökerenként egy példányt ad a folyamatban (`Arc<Mutex<ProjectGraph>>`). A CLI-ben ez egy
hívás; az **MCP-szerver** (`callgraph mcp`) hosszan fut, ott a manifest, a szűkített tények, a hívó-index és a már
betöltött teljes rekordok kérésről kérésre megmaradnak, és kérésenként csak a változott fájlok frissülnek.

## Kimenet és összevetés

A gráfból kiszolgált lekérdezés kimenete bájtra azonos a gráf nélküli úttal: `CALLGRAPH_NO_GRAPH=1` — ilyenkor a
`show` minden fájlt elemez (cache nélkül), a `--depth` hívó-indexe a teljes projekt bejárásából épül. A
`tests/project_graph.rs` több nyelvű projekten veti össze a kettőt (hidegen, melegen, szerkesztés, törlés és sérült
tároló után). Írásvédett projektben (a `.callgraph/` nem hozható létre) a `show` automatikusan a gráf nélküli úton
fut.

Időmérés egy valódi projekten: `bash tools/graph_bench.sh <projekt> [<hatókör> [<minta> [<ismétlés>]]]`
(`CALLGRAPH_BIN=…` a mérendő bináris); az eredmény csak a terminálra kerül.
