# callgraph

PHP / Rust / JS / TS / Vue / Swift kód-térkép agenteknek: a fájl beolvasása nélkül ad szerkezetet, hívás-gráfot, metódus-törzset,
„ki hívja” és hatáselemzést. Az elemző egy önálló Rust-bináris (`callgraph`, forrás: callgraph); a `callgraph.sh` vékony burok fölötte.
A használat leírása a `SKILL.md`-ben van.

## Csomag

`SKILL.md`, `README.md`, `TELEPITES.md` (lépésenkénti telepítés), `install.sh`, `callgraph.sh`, `lib.sh`, `projects.sh`, `feedback.sh`, `extras/`, `help/` (a scriptek súgója angolul és magyarul),
`bin/<os>-<arch>/callgraph`.

## Telepítés

1. Csomagold ki a skill-mappába (pl. `~/.claude/skills/callgraph/`).
2. Futtasd: `bash install.sh` — beállítja a jogosultságokat és ellenőrzi a függőségeket.
3. Regisztrálj egy projektet: `bash projects.sh add myapp /abszolút/út/a/projekthez`.
4. Próba: `bash callgraph.sh --project myapp app/Models/User.php`.

- A `bin/<os>-<arch>/callgraph` a platformhoz illő előre fordított bináris (pl. `darwin-arm64`, `linux-x86_64`).
  Más platformra forrásból kell fordítani.
- macOS: a csomag Apple Silicon (`darwin-arm64`) és Intel (`darwin-x86_64`) binárist is tartalmaz (ad-hoc aláírva); az `install.sh` leveszi a letöltés karantén-jelzőjét.
- Linux: `linux-aarch64` és `linux-x86_64` statikus (musl) bináris — bármely disztribúción fut, külső függőség nélkül (a burokhoz csak `bash` kell).
- Forrásból (Rust 1.85+): `cargo build --release`, majd a `target/release/callgraph` másolata a
  `bin/<os>-<arch>/callgraph` helyre (`<os>` = `uname -s` kisbetűvel, `<arch>` = `uname -m`).
- Windows: Git Bash kell (a `uname` `MINGW64_NT-…` értékét a burok `windows`-ra normalizálja); a csomag `bin/windows-x86_64/callgraph.exe` binárist tartalmaz.
- Külső függőség nincs (se PHP, se Node): az elemzést, a projekt-aliasokat és a feedbacket a `callgraph` végzi; a burokhoz csak `bash` kell.

## Nyelv

A terminál-kimenet és a súgó angol (alapértelmezés) vagy magyar. Váltás: `bash callgraph.sh set-lang hu` (vissza: `set-lang en`,
lekérdezés: `set-lang` argumentum nélkül) — a skill-mappa `lang` fájljába menti, frissítéskor megmarad. Egy futásra: `CALLGRAPH_LANG=hu bash callgraph.sh …`
(a környezeti változó erősebb az elmentett beállításnál).

A nyelvváltás a skill-utasítást is cseréli: a `set-lang` a `skill-i18n/SKILL.<en|hu>.md`-ből újraírja a `SKILL.md`-t
(a másik nyelvű eltűnik); az első telepítés (`install.sh` → `callgraph skill-sync`) és a frissítés is az érvényes nyelvűt
hagyja ott. Az új utasítás betöltéséhez indítsd újra a Claude Code-ot. A `SKILL.md` generált fájl — a `skill-i18n/` forrásokat szerkeszd.

## Frissítés

`bash callgraph.sh update --check` megnézi, van-e újabb kiadás a GitHub Releases-en; `bash callgraph.sh update` letölti a
platform csomagját (`callgraph-<os>-<arch>.zip`) és a skill-mappába telepíti: a skill-fájlok és a `bin/` frissülnek, a
`projects.json`, a `lang`, az `errors.jsonl`, a `feedback.jsonl`, az `update-check.json` és a csomagban nem szereplő fájlok érintetlenek.
`--force`: újratelepítés akkor is, ha naprakész; `--from <zip|URL>`: helyi zipből / tükörről. A verzió: `callgraph --version`.
Ellenőrzés: a letöltött zip SHA-256-ját a kiadás `SHA256SUMS` fájljához veti; eltérésnél vagy ha a kiadásnak nincs ilyen
fájlja (v0.1.0), semmit nem telepít — `--insecure-skip-checksum` kihagyja az ellenőrzést. `--from` mellett a
`--sha256 <hex>` vagy a zip melletti `SHA256SUMS` szerint ellenőriz; ha egyik sincs, egysoros figyelmeztetéssel telepít.
Az `update --check` az `update-check.json`-t is frissíti (a napi háttér-ellenőrzés nem ismétli meg).
Csak skill-telepítésben fut (a bináris a `<skill>/bin/<os>-<arch>/` alatt); egy `target/release/callgraph`-ot nem ír felül.
A legutóbbi kiadást a `github.com/GeRiY/callgraph/releases/latest` átirányításából olvassa ki (nincs API-korlát; tartalék: a GitHub API).
A kérések gyorsítótár-kerülők; ha a válasz „naprakész”, a kiadás-lista (`/releases?per_page=5`) legújabb stabilja is
számít — így egy friss kiadás után sem mond percekig „naprakész”-t.
Dev-csatorna: `update --dev` / `update --check --dev` — a `develop` ág gördülő `dev` előzetes kiadása
(`X.Y.Z-dev.<sha7>`, ugyanúgy `SHA256SUMS`-szal ellenőrizve; tükör: `CALLGRAPH_UPDATE_DEV_API`). A sima `update`
dev-buildről a legutóbbi stabilra vált; a háttér-ellenőrzés és az MCP `--auto-update` csak stabil kiadást ajánl / telepít
(dev-buildről csak újabb alapverziót).

Automatikus ellenőrzés: a `callgraph.sh` naponta legfeljebb egyszer, várakozás nélkül egy háttérfolyamatot indít, ami
megnézi a legutóbbi kiadást (állapot: `update-check.json` a skill-mappában); ha újabb van, a kimenet végére egy sor
kerül (`ℹ callgraph X is available … — update: bash …/callgraph.sh update`), verziónként legfeljebb naponta egyszer.
Kikapcsolás: `CALLGRAPH_UPDATE_CHECK=0`; `CI` mellett és skill-telepítésen kívül magától nem fut.

## Környezeti változók

| Változó | Hatás |
|---|---|
| `CALLGRAPH_BIN` | az `callgraph` bináris útja (alapból `bin/<os>-<arch>/callgraph`, majd `bin/callgraph`) |
| `CALLGRAPH_REGISTRY` | a projekt-alias fájl (alapból a skill `projects.json`-ja) |
| `CALLGRAPH_OUT_LIMIT` | a kimenet-korlát sorszáma (alapból 400) |
| `PHP_AST_HOME` | a hibaüzenetekben mutatott skill-út, ha a skill máshová van linkelve |
| `CALLGRAPH_LANG` | a terminál-kimenet és a súgó nyelve (`en` / `hu`; pl. `hu_HU.UTF-8` is jó) — felülírja az elmentett beállítást |
| `CALLGRAPH_UPDATE_API` | API-formátumú (`releases/latest` JSON) kiadás-forrás az `update`-hez és az automatikus ellenőrzéshez (alapból a `github.com/…/releases/latest` átirányítása, tartalékként az API; tükör / teszt, `file://` is lehet; beállítva a kiadás-lista tartalék-lekérdezése elmarad) |
| `CALLGRAPH_UPDATE_DEV_API` | API-formátumú (`releases/tags/dev` JSON) dev-kiadás-forrás az `update --dev`-hez (alapból `api.github.com/repos/GeRiY/callgraph/releases/tags/dev`; tükör / teszt, `file://` is lehet) |
| `CALLGRAPH_UPDATE_CHECK` | `0` / `off` / `false`: kikapcsolja az automatikus, napi frissítés-ellenőrzést (a `CI` beállítása is kikapcsolja) |

Az érték elöl álló argumentumként is megadható: `bash callgraph.sh CALLGRAPH_OUT_LIMIT=100 …`.

## Közvetlen `callgraph`-parancsok

```bash
callgraph <fájl|mappa>...                    # elemzés → JSONL, fájlonként egy sor
callgraph show --project <gyökér> [kapcsolók] <fájl|mappa>[::metódus]   # a lekérdező mag
callgraph wrap [--project alias] [kapcsolók] <fájl|mappa>[::metódus]   # amit a callgraph.sh hív (alias + error-id)
callgraph project add|list|remove|resolve|get|set-config …              # amit a projects.sh hív
callgraph feedback add|resolve|list|show …                              # amit a feedback.sh hív
callgraph callers <minta> --depth N          # a hívók hívói N szintig
callgraph set-lang [en|hu]                  # a kimenet nyelve (argumentum nélkül: az aktuális)
callgraph self-update [--check|--force] [--from <zip|URL> [--sha256 <hex>]] [--insecure-skip-checksum]   # amit a callgraph.sh update hív
callgraph --version                         # verzió és platform (pl. callgraph 0.1.0 (linux-x86_64))
callgraph prune [--dry-run]                  # a cache-ből törli a nem létező fájlok és a régi verzió bejegyzéseit
callgraph --lang php,css <fájl|mappa>…       # csak a megadott nyelvek fájljait elemzi (a többit kihagyja)
```

A cache a projekt `.callgraph/` mappájában él (`*`-os `.gitignore`-ral); `--no-cache` kikapcsolja.
Kilépési kód: `0` siker, `1`, ha valamelyik fájl elemzése hibázott (a többi sor ettől még kiíródik).

## Ismert korlátok

- A PHP- és a JS/TS/Vue-elemzés a korábbi (PHP/Node) motor kimenetével ~100%-ban egyezik; a maradék néhány
  kommentsor és `--depth --guess` találgatás.
- A `jobs` nézet szándékosan a javított reguláris kifejezést használja: a régi motor mintája hibás volt,
  és mindig üres eredményt adott.
- Statikus elemzés: dinamikus hívás és magic method nem oldható fel.
- Hibánál a `feedback.sh` gyűjti a visszajelzést az `[error-id]` megadásával.
