# Token-mérés: callgraph vs. Read/Grep

`tools/token_bench.py` (csak Python 3 stdlib) azt méri, mennyi kontextust (token), hány eszközhívást (lépés) és mennyi
időt kíván egy kódkérdés megválaszolása egy agentnek **callgraph-fal** és **nélküle** (Read + grep).
A korpuszokat csak olvassa (grep, fájlolvasás, `callgraph show`); belőlük semmit nem futtat.

## Futtatás

```bash
python3 -I tools/token_bench.py --bin target/docker/x86_64-unknown-linux-musl/release/callgraph \
  --corpus laravel:php:/munka/laravel-framework:src \
  --corpus koel-php:php:/munka/koel:app \
  --corpus koel-ts:ts:/munka/koel:resources/assets/js \
  --corpus axum:rust:/munka/axum:. \
  --corpus ripgrep:rust:/munka/ripgrep:crates \
  --corpus callgraph-src:rust:/munka/callgraph-src:src \
  --corpus swift-samples:swift:/munka/samples/swift:. \
  --runs 5 --out eredmeny.json --html riport.html [--report-text szovegek.json]

# csak a riport újragenerálása egy meglévő eredményből (mérés nélkül):
python3 -I tools/token_bench.py --render eredmeny.json --html riport.html --report-text szovegek.json
```

`--corpus <név>:<nyelv>:<gyökér>:<almappa>`, nyelv: `php | rust | ts | swift | scss` (a `ts` a .ts/.tsx/.js/.jsx/.vue fájlokat
jelenti). A callgraph a `<gyökér>/.callgraph/` cache-t írja (a hideg mérés törli), ezért a korpuszból **munkamásolatot** adj meg.
A `--report-text` JSON a riport kézzel írt szövegei (`headline`, `method`, `limitations` HTML-ként, `notes` feladatonként).

## Feladatok (a szkript választja, determinisztikusan, a valós kódból)

Korpuszonként; a teszt-, spec-, bench-, example- és `.d.ts` fájlok nem lehetnek célpontok.

| Típus | Kérdés | Választás |
|---|---|---|
| `outline` | „Mi van ebben a fájlban?” | a ≤ 2000 soros, ≥ 3 definíciós fájlok közül a méret szerinti 10. és 33. percentilis |
| `api` | „Mi a fájl publikus API-ja?” | a 20. percentilis |
| `body` | „Mit csinál X?” (ismert fájl) | 15–90 soros függvény ≥ 250 soros fájlban (zárójel-párosítással mérve), a legnagyobb fájlokból |
| `callers` | „Ki hívja X-et?” | egyszer definiált, nem triviális név (stoplista), a legtöbb (4–20) hívó fájllal |
| `callers_d2` | hatáselemzés, 2 szint | 3–8 hívó fájlos név |
| `strings` | „Hol szerepel ez a szöveg?” | ≥ 12 karakteres, szóközös literál 2–6 termékkód-fájlban |
| `const` | „Hol definiálják / használják a konstanst?” | egyszer definiált PHP-osztálykonstans / Rust `const`, 3–12 fájlban |

Kis korpuszon (< 30 fájl, pl. a golden-minták) a küszöbök lazábbak.

## Stratégiák

- **Naiv baseline:** outline/api/body → a teljes fájl Read-je; hívók → `grep -rnE '\bX\s*\('` + minden hívó fájl teljes Read-je;
  2. szint → az 1. szint hívó függvényeinek (a legközelebbi megelőző definíció) neveire újabb grep + a még nem olvasott fájlok;
  string/konstans → grep + a találati fájlok teljes Read-je.
- **Okos baseline:** body → `grep -n 'function X'` + Read offset/limit **pontosan** a függvény sávjával (alsó korlát: egy valós agent
  a végét nem tudja előre); hívók → grep + találatonként ±30 sor (az átfedő ablakok összevonva, ablakonként egy Read);
  string/konstans → egyetlen `grep -C3`.
- **callgraph:** egy `callgraph show …` hívás, a pontos parancs és a teljes stdout a JSON-ban. Hívóknál az elsődleges minta név
  szerinti (`(^|::|\.|->)X$`); a SKILL.md-stílusú `Típus::X$` minta külön is lefut (`callgraph_qualified`).
- **Tényleges költség:** ha a callgraph-válasz a helyességi kritérium szerint hiányos, az agentnek a grep-es úton is végig kell
  mennie: tényleges token = callgraph + okos baseline, lépés = 1 + okos lépések. A fő mutató ez (`x_eff_smart_*`).
- **„Hogyan hívják?” (v2):** az okos baseline ±30 soros ablakai a hívás sorát is megmutatják. Ha a callgraph hívó-kimenetében
  nincs hívássor (`L<n> │ …`), az agentnek még egy `--grep … --body` kell: ennek tokenje és +1 lépés a callgraph-oldalra kerül
  (`callgraph_followup`). Így a régebbi, hívássor nélküli verziók is azonos mércével mérhetők.

A Read kimenete `cat -n` formátum (6 széles sorszám + tab), a grep-é `út:sor:szöveg`, a gyökérhez relatív utakkal.

## Mérőszámok

- **Token:** a Claude tokenizere helyben nem érhető el (tiktoken sincs), ezért két közelítés: `tok_b4 = ⌈bájt/4⌉` (a fő) és
  `tok_words` = `[A-Za-z]+ | \d+ | írásjel` darabszám. A × arányok a kettővel nagyságrendben egyeznek; abszolút értéknek
  egyik sem pontos. Az eszközhívásonkénti fix többlet (a hívás maga, kb. 30–80 token) nincs benne — ez a lépésszámon látszik.
- **Lépés:** eszközhívások száma.
- **Idő:** a parancsok saját futásideje, `--runs` futás mediánja; callgraph **hideg** (a `.callgraph/` törölve minden futás előtt) és
  **meleg** cache-sel. A modell körútja (lépésenként jellemzően másodpercek) NINCS benne — a valós falióra-különbséget a lépésszám dönti el.
- **Helyesség** (a grep a viszonyítás, nem tökéletes igazság). **v2:** a grep-találatok **komment-szűrtek** — egy fájl-szintű
  állapotgép (stringek, `//`, `/* … */` több soron át, `/*! … */` doc-blokk, PHP `#`) kivágja a megjegyzéseket, és csak az a
  találat számít, ahol a név / szöveg a kódban van (a nyers szám `grep_files_raw` néven megmarad):
  outline/api — a regexszel talált (api-nál a publikus) definíciók hány %-a látszik (≥ 90%);
  body — a függvény nem üres **kód**sorainak hány %-a van a kimenetben (≥ 98%; a megjegyzés-sorok nem kötelezők);
  hívók — a grep-pel talált hívó fájlok közül hányat ad vissza a callgraph (≥ 90%), és a grep-sorok hány %-át fedi le metódus-sáv;
  string/konstans — fájlszintű visszahívás a grep-hez képest (≥ 90%).

## Eredmények

A mérési eredmények (számok, riport) **nem kerülnek a repóba**: a `--out` / `--html` fájlokat a repón kívülre írd.
