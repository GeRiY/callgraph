# Projekt-gráf (`.callgraph/index.jsonl`)

A projekt-szintű lekérdezések (`--grep`, `--depth`, `--strings`, `--const`, a `callers` parancs és az MCP-szerver
ugyanezen eszközei) nem járják be és nem elemzik újra a projekt fájljait minden hívásnál: egy **perzisztens
projekt-gráfból** olvasnak, amely a projekt `.callgraph/` tárolójában él, és fájlonként frissül.

Kód: `src/graph/` (`ProjectGraph`, tárolt formátum: `record.rs`, folyamaton belüli példány: `pool.rs`).

## Mit tárol

Fájlonként egy rekordot:

- a nyelvi frontend **`FileGraph`-ját változatlanul** — konténerek, metódusok / függvények sortartománnyal,
  konstansok, hívások (feloldott céllal), string-literálok, öröklés (`extends` / `implements` / `traits`) …;
- a fájl **hívás-sorainak tömörített forrássorát** (`(sor, kód)` párok, ugyanaz a szöveg, mint a `L25 │ …`
  jelölésben), hogy a hívó-találatok kiírásához ne kelljen a forrásfájlt újraolvasni;
- a változás-bélyeget: mtime (ns) + méret.

Ebből épül, a memóriában, a projekt-szintű nézet:

| Nézet | Hol | Mire |
|---|---|---|
| szimbólum-keresés (definíciók sortartománnyal) | `ProjectGraph::method_range` / a tárolt `FileGraph`-ok | a `--depth` hívó-sorainak `L<n>-<m>` tartománya |
| hívás-élek mindkét irányban | `index::CallerIndex` (előre: a fájl hívásai; hátra: cél → hívók) | `--depth` 2+. szint, `callers` |
| típus-hierarchia | `index::TypeHierarchy` (a `CallerIndex` része) | `via <Típus>` illesztés |
| string-literálok, konstans-hivatkozások | a hatókör fájljainak tárolt `FileGraph`-ja | `--strings`, `--const` |

A származtatott nézetek szándékosan nincsenek külön tárolva: fájlokon átívelők (egy interfész változása máshol is
módosítja a `via` illesztést), a tárolt `FileGraph`-okból forrás-elemzés nélkül, memóriában épülnek. Egy új
`FileGraph`-mező (pl. egy frontend új adata) így gráf-kódváltozás nélkül bekerül a tárolóba; a lekérdezés, amelyik
olvassa, a gráfból kapott `FileGraph`-ban látja.

## Formátum

JSON Lines (a projekt többi rekordlistájához hasonlóan). Az első sor a fejléc:

```json
{"format":"callgraph-project-graph","version":1,"tool":"0.2.0+s1+<forrás-ujjlenyomat>"}
```

- `format`: rögzített azonosító (a régi, fejléc nélküli `callers`-indexet ebből ismerjük fel);
- `version`: a rekord-szerkezet verziója (`record::FORMAT_VERSION`);
- `tool`: `io::tool_version()` — verzió + séma + a forrás ujjlenyomata. Egy új build (bármelyik nyelvi frontend
  változása) így a régi elemzéseket nem használja újra.

Utána fájlonként egy sor, a fej előbb (betöltéskor csak ezt értelmezzük; a gráfot csak akkor, ha egy lekérdezés
kéri):

```json
{"path":"/abs/projekt/src/Repo.php","mtime":1730800000123456789,"size":812,"snippets":[[12,"return $this->repo->find($id);"]],"graph":{ … FileGraph … }}
```

A sorok út szerint rendezve; az írás ideiglenes fájlba, majd átnevezéssel történik (egy félbeszakadt írás nem
rontja el). Csak akkor ír, ha változott valami.

## Frissítés és érvénytelenítés

- Minden lekérdezés a saját fájljaira megnézi a bélyeget (stat, nem olvasás): változatlan fájl → a tárolt gráf;
  változott → újraelemzés; új fájl → előbb a fájlonkénti cache (`cache.jsonl`), különben elemzés.
- A `--depth` 2. szintje és a `callers` a teljes projektet nézi: a bejárásban nem szereplő (törölt, átnevezett)
  fájlok kikerülnek a gráfból.
- Hiányzó, sérült, régi formátumú vagy más build által írt fájl → üres gráf, a következő lekérdezés csendben
  újraépíti. Egy sérült rekord csak a saját fájlját elemezteti újra.
- A hívó-index csak akkor épül újra, ha egy fájl vagy a fájl-sorrend változott.

## Folyamaton belül: egy példány gyökerenként

`graph::shared(gyökér)` gyökerenként egy példányt ad a folyamatban (`Arc<Mutex<ProjectGraph>>`). A CLI-ben ez egy
hívás; az **MCP-szerver** (`callgraph mcp`) hosszan fut, ott a betöltött gráf és a hívó-index kérésről kérésre
megmarad, és kérésenként csak a változott fájlok frissülnek (a lemezre írás ugyanúgy megtörténik, a CLI-vel közös).

## Kimenet

A gráfból kiszolgált lekérdezés kimenete bájtra azonos a fájlonkénti úttal. Kapcsoló az összevetéshez és
hibakereséshez: `CALLGRAPH_NO_GRAPH=1` — ilyenkor a `show` a régi úton fut (a hatókör fájljai a cache-ből / elemzésből,
a `--depth` hívó-indexe minden hívásnál a teljes projekt bejárásából). A `tests/project_graph.rs` több nyelvű
projekten veti össze a kettőt (hidegen, melegen, szerkesztés és törlés után). Írásvédett projektben (a `.callgraph/`
nem hozható létre) a `show` automatikusan a régi úton fut.

A fájlonkénti nézetek (outline, `--body`, `--lines`, framework-nézetek) maradtak a fájl-alapú úton (cache).
