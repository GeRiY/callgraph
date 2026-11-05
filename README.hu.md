<h1 align="center">callgraph</h1>

<p align="center">
  <b>Token-hatékony kód-térkép AI kódoló agenteknek.</b><br>
  Szerkezet, kategorizált hívás-gráf, metódus-törzs, „ki hívja?” és hatáselemzés —
  a teljes fájlok beolvasása nélkül.
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

| Nyelv | Kiterjesztések | Mit kapsz |
|---|---|---|
| PHP | `.php` | osztályok, traitek, enumok, metódusok, property-k, konstansok, hívások típuskövetéssel, framework-felismerés (Laravel, Yii) |
| JavaScript / TypeScript | `.js` `.mjs` `.cjs` `.jsx` `.ts` `.tsx` | modulok, függvények, osztályok, importok alias-feloldással (`jsconfig` / `tsconfig`) |
| Vue | `.vue` | props, emits, `setup`, template-komponensek és eseménykezelők, `computed`, `watch`, életciklus-hookok, Options API |
| CSS / SCSS | `.css` `.scss` | szabály-blokkok, mixinek, függvények, változók, `@use` / `@import` |
| JSON / Markdown | `.json` `.md` | kulcs-útvonalak; címsor-szakaszok és linkek |

### Mikor *ne* használd

- Egy ismert sorsáv pontos szövege kell: `--lines` vagy célzott olvasás.
- Nem kód fájlok (sima szöveg, adat-dump): az outline itt keveset ad.
- Makró- vagy reflexió-nehéz kód: a statikusan nem feloldható, dinamikus hívások csak név szerinti egyezésként (`?`,
  `--guess`-szel) jelennek meg, vagy sehogy.

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

## Használat

Minden parancs a `callgraph.sh`-n át megy (vagy közvetlenül a binárissal: `callgraph show …`). A `--project <alias>`
mellett az utak a regisztrált projekt gyökeréhez képest értendők; nélküle az aktuális mappa a gyökér.

| Feladat | Parancs |
|---|---|
| Fájl / mappa outline-ja | `callgraph.sh <fájl\|mappa>` |
| Csak egy metódus | `callgraph.sh <fájl>::<metódus>` (több szelektor is mehet) |
| Metódus forrása sorszámozva | `callgraph.sh <fájl>::<metódus> --body` |
| Tetszőleges sorsáv | `callgraph.sh <fájl> --lines 30-60` |
| Hol hívják? | `callgraph.sh <mappa> --grep <minta>` |
| Hatáselemzés | `callgraph.sh <mappa> --grep <minta> --depth N [--guess]` |
| String-literál / konstans helye | `callgraph.sh <mappa> --strings <minta>` · `--const <minta>` |
| Szűrés hívás-kategóriára | `callgraph.sh <mappa> --category model_use,orm_relation` |

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

### Nyelv

A terminál-kimenet és a súgó alapból angol; magyarul is kérhető:

```bash
bash ~/.claude/skills/callgraph/callgraph.sh set-lang hu    # elmentés (vissza: set-lang en)
bash ~/.claude/skills/callgraph/callgraph.sh set-lang       # az aktuális nyelv és a forrása
CALLGRAPH_LANG=hu bash ~/.claude/skills/callgraph/callgraph.sh app/Models/Order.php   # csak egy futásra
```

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
           vendor, …)      szerint)           pulldown-cmark)          séma)
```

- Minden nyelv egy modul a `src/lang/<nyelv>/` alatt, ami az AST-jét egy közös `FileGraph` sémára képezi le; a
  lekérdező réteg (`src/query/`) nem tudja, melyik nyelvből jött egy fájl.
- Parserek: [mago](https://github.com/carthage-software/mago) (PHP), [oxc](https://github.com/oxc-project/oxc)
  (JS / TS / Vue), [raffia](https://github.com/g-plane/raffia) (CSS / SCSS),
  [pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark) (Markdown).
- Az eredmények fájlonként JSONL-be cache-elődnek (mtime-alapon); a „ki hívja?” egy inkrementális projektindexet
  (mtime + méret) használ a `.callgraph/` alatt.

Felépítés és kódolási szabályok: [`ARCHITECTURE.md`](ARCHITECTURE.md) · lekérdező réteg: [`docs/QUERY.md`](docs/QUERY.md) ·
nyelvenkénti leképezés: [`docs/LANG-JS.md`](docs/LANG-JS.md), [`docs/LANG-CSS.md`](docs/LANG-CSS.md),
[`docs/LANG-DOC.md`](docs/LANG-DOC.md).

## Fejlesztés

Helyben csak Docker kell; a toolchain (Rust stable, rustfmt, clippy, musl target, python3, php) a fejlesztői
image-ben van (`docker/dev.Dockerfile`):

```bash
bash tools/dev.sh check     # fmt --check + clippy -D warnings + tesztek — minden commit előtt
bash tools/dev.sh build     # target/docker/release/callgraph
bash tools/dev.sh test      # csak a tesztek
bash tools/dev.sh skill     # dist/callgraph-<verzió>.tar.gz (a skill-csomag, Linux)
bash tools/dev.sh shell     # shell a konténerben
```

Helyi toolchainnel a szokásos `cargo build --release` / `cargo test --release` is működik. A CI
([`.github/workflows/release.yml`](.github/workflows/release.yml)) minden pushnál buildel és tesztel macOS-re
(arm64, x86_64), Linuxra (arm64, x86_64, statikus musl) és Windowsra, `v*` tagnél pedig GitHub Release-t készít.

### Közreműködés

Issue-kat és pull requesteket szívesen fogadunk. PR előtt futtasd a `bash tools/dev.sh check`-et.
Új nyelv = egy új modul + egy regisztrációs sor: lásd [`docs/UJ-NYELV.md`](docs/UJ-NYELV.md).
Refaktornál a kimenetnek bájtra azonosnak kell maradnia (`tools/identity_check.sh`).

### Biztonság

A sérülékenységeket privátban jelezd, a
[GitHub Security Advisories](https://github.com/GeRiY/callgraph/security/advisories/new) oldalon, ne nyilvános issue-ban.

## Licenc

[MIT](LICENSE)
