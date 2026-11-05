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

A Read kimenete `cat -n` formátum (6 széles sorszám + tab), a grep-é `út:sor:szöveg`, a gyökérhez relatív utakkal.

## Mérőszámok

- **Token:** a Claude tokenizere helyben nem érhető el (tiktoken sincs), ezért két közelítés: `tok_b4 = ⌈bájt/4⌉` (a fő) és
  `tok_words` = `[A-Za-z]+ | \d+ | írásjel` darabszám. A × arányok a kettővel nagyságrendben egyeznek; abszolút értéknek
  egyik sem pontos. Az eszközhívásonkénti fix többlet (a hívás maga, kb. 30–80 token) nincs benne — ez a lépésszámon látszik.
- **Lépés:** eszközhívások száma.
- **Idő:** a parancsok saját futásideje, `--runs` futás mediánja; callgraph **hideg** (a `.callgraph/` törölve minden futás előtt) és
  **meleg** cache-sel. A modell körútja (lépésenként jellemzően másodpercek) NINCS benne — a valós falióra-különbséget a lépésszám dönti el.
- **Helyesség** (a grep a viszonyítás, nem tökéletes igazság):
  outline/api — a regexszel talált (api-nál a publikus) definíciók hány %-a látszik (≥ 90%);
  body — a függvény nem üres sorainak hány %-a van a kimenetben (≥ 98%);
  hívók — a grep-pel talált hívó fájlok közül hányat ad vissza a callgraph (≥ 90%), és a grep-sorok hány %-át fedi le metódus-sáv;
  string/konstans — fájlszintű visszahívás a grep-hez képest (≥ 90%).

## Eredmény (2026-11-05, develop @ 0baa09f, musl release-build)

Korpuszok: laravel/framework `src` (1748 PHP-fájl), koel `app` (1067 PHP) és `resources/assets/js` (1014 TS/Vue), axum (305 .rs),
ripgrep `crates` (90 .rs), a callgraph saját `src`-je (266 .rs), `golden/samples/swift` (4 fájl). 60 feladat, 5 futás.

| Típus | n | × okos (tényleges) | × naiv | lépés okos → cg | cg meleg / hideg (ms) | helyes |
|---|---|---|---|---|---|---|
| outline | 13 | 4,5× | 4,5× | 1 → 1 | 7 / 20 | 12/13 |
| api | 7 | 7,7× | 7,7× | 1 → 1 | 6 / 26 | 6/7 |
| body | 12 | **0,96×** | 18,1× | 2 → 1 | 10 / 49 | 11/12 |
| callers (1 szint) | 12 | 10,7× | 43× | 19 → 1 | 132 / 1200 | 12/12 |
| callers_d2 | 6 | 55× | 130× | 53,5 → 1 | 370 / 1484 | 6/6 (1. szint) |
| strings | 5 | 2,6× | 6,1× | 1 → 1 | 238 / 1144 | 3/5 |
| const | 5 | 1,0× | 26,9× | 1 → 2 | 148 / 1031 | 2/5 |
| **összes** | 60 | **4,5×** (Σ-arány 6,4×) | 17,7× | 1,5 → 1 | 30 / 70 | 52/60 |

Tanulságok (részletesen a riport „Korlátok” részében):

- A hívó-keresés és a hatáselemzés a nagy nyereség; a 2. szint viszont mindkét oldalon pontatlan (a grep név-ütközésekkel
  felfújt, a callgraph az interfészen át hívott metódust nem köti be, Rustban a feloldatlanokat `--guess` nélkül kihagyja).
- A metódus-törzs **nem** olcsóbb a célzott Read-nél (a docblock miatt 0,82–1,03×); a nyereség csak a lépés.
- A SKILL.md-stílusú `Típus::metódus` minta 14 hívó-feladatból 12-ben legfeljebb 1 hívó fájlt adott (Rust: fogadón át hívott
  metódus; PHP: trait- és örökölt metódus a hívott osztály nevén); a név szerinti minta mindenhol teljes volt.
- `--const` Rust `const`-ra nem ad találatot; `--strings` nem látja a Vue-template `{{ }}`-beli és a Rust `const`-beli stringeket;
  Rust `--format api` az `impl`-ek `pub fn`-jeit nem listázza; JS/TS objektum-literálos metódus nem külön metódus.
- A keresés-kimenetek bájtjainak ~17%-a az abszolút gyökér-előtag.
