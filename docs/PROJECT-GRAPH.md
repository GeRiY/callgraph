# Projekt-gráf (`.callgraph/`)

Minden `show`-lekérdezés (outline, `--body`, `--grep`, `--depth`, `--strings`, `--const` …), a `callers` parancs és
az MCP-szerver ugyanezen eszközei egy **perzisztens projekt-gráfból** olvasnak, amely a projekt `.callgraph/`
tárolójában él és fájlonként frissül. A fájlonkénti shardok egyben az elemzési cache is (a régi `cache.jsonl`
helyett).

Kód: `src/graph/` — `ProjectGraph` (`store.rs`), a bináris formátum (`codec.rs`), a fájlonkénti adatok
(`record.rs`), a folyamaton belüli példány (`pool.rs`).

## A tároló

```
.callgraph/
  index.bin          manifest: forrásfájlonként út, mtime (ns), méret, shard-név
  graph/<név>.bin    fájlonként egy shard: a FileGraph (veszteségmentesen) + a hívás-sorok
  callers.bin        a hívó-index bemenete: fájlonként a hívások, konténerek, metódusok + hívás-sorok
  notes.jsonl        a jegyzetek (külön funkció, a gráf nem nyúl hozzá)
  .gitignore         `*`
```

- **Shard-név:** az út FNV-1a 64 bites kivonata (`<16 hex>.bin`) — stabil, két folyamat ugyanannak a fájlnak
  ugyanazt a shardot írja. A shard a saját útját és bélyegét is tárolja; ha nem egyezik a manifesttel, érvénytelen.
- **`callers.bin`:** fájlonként a `FileGraph` szűkített alakja (`FileFacts::slim`: névtér, konténerek, metódusok,
  hívások argumentumok nélkül) és a hívás-sorok. Ebből épül memóriában a fordított hívó-index (`index::CallerIndex`),
  a típus-hierarchia (`index::TypeHierarchy`, a `via` illesztéshez) és a metódus-tartományok (`L<n>-<m>`); a `--depth`
  és a `callers` így egyetlen shardot sem olvas.

## Formátum

Minden fájl keretezett: `CGB1`, utána keretek: `[u32 LE hossz][bájtok]`. Az első keret a fejléc (MessagePack):

```
{ kind: "callgraph-manifest" | "callgraph-shard" | "callgraph-callers", version: 2, tool: "<io::tool_version()>" }
```

- `version`: a tárolt szerkezet verziója (`codec::FORMAT_VERSION`);
- `tool`: verzió + séma + a forrás ujjlenyomata — egy új build (bármelyik nyelvi frontend változása) a régi
  elemzéseket nem használja újra.

A többi keret MessagePack (mezőnevekkel), LZ4-gyel tömörítve (`rmp-serde`, `lz4_flex`; tiszta Rust). A manifestnek
egy kerete van (a bejegyzések listája), a shardnak egy (a fájl rekordja), a `callers.bin`-nek fájlonként egy (így
párhuzamosan dekódolható, és egy fájl változásakor a többi keret kódolás nélkül visszaíródik).

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
  (párhuzamosan), és csak az ő shardjuk íródik újra; a `callers.bin`-ben csak az ő bemenetük cserélődik.
- A teljes projekt-listában nem szereplő (törölt, átnevezett) fájlok és shardjaik kikerülnek.
- Az 1. szintű `--grep`, a `--strings`, a `--const`, az outline és a `--body` csak a hatókör fájljainak shardjait
  tölti be (párhuzamosan).
- Hiányzó, sérült vagy más build által írt manifest → a tároló (shardok, `callers.bin`) törlődik és csendben
  újraépül. Sérült shard → csak az a fájl elemződik újra. Sérült `callers.bin` (vagy keret) → a bemenet a
  shardokból áll vissza.
- **Átállás:** a korábbi tárolók (`index.jsonl` — a v0.3 előtti JSONL gráf, `cache.jsonl` — a v0.2 fájlonkénti
  JSON-cache) az új tároló első építésekor törlődnek. (Az alacsony szintű `analyze` parancs a saját JSON-cache-ét
  továbbra is használhatja; a lekérdezések nem.)
- Minden írás atomikus: ideiglenes fájl (folyamat + számláló), majd átnevezés.

## Folyamaton belül: egy példány gyökerenként

`graph::shared(gyökér)` gyökerenként egy példányt ad a folyamatban (`Arc<Mutex<ProjectGraph>>`). A CLI-ben ez egy
hívás; az **MCP-szerver** (`callgraph mcp`) hosszan fut, ott a manifest, a hívó-index és a már betöltött shardok
kérésről kérésre megmaradnak, és kérésenként csak a változott fájlok frissülnek.

## Kimenet és összevetés

A gráfból kiszolgált lekérdezés kimenete bájtra azonos a gráf nélküli úttal: `CALLGRAPH_NO_GRAPH=1` — ilyenkor a
`show` minden fájlt elemez (cache nélkül), a `--depth` hívó-indexe a teljes projekt bejárásából épül. A
`tests/project_graph.rs` több nyelvű projekten veti össze a kettőt (hidegen, melegen, szerkesztés, törlés és sérült
tároló után). Írásvédett projektben (a `.callgraph/` nem hozható létre) a `show` automatikusan a gráf nélküli úton
fut.

Időmérés egy valódi projekten: `bash tools/graph_bench.sh <projekt> [<hatókör> [<minta> [<ismétlés>]]]`
(`CALLGRAPH_BIN=…` a mérendő bináris); az eredmény csak a terminálra kerül.
