# callgraph

PHP / JS / TS / Vue kód-térkép agenteknek: a fájl beolvasása nélkül ad szerkezetet, hívás-gráfot, metódus-törzset,
„ki hívja” és hatáselemzést. Az elemző egy önálló Rust-bináris (`callgraph`, forrás: callgraph); a `callgraph.sh` vékony burok fölötte.
A használat leírása a `SKILL.md`-ben van.

## Csomag

`SKILL.md`, `README.md`, `install.sh`, `callgraph.sh`, `lib.sh`, `projects.sh`, `feedback.sh`, `extras/`,
`bin/<os>-<arch>/callgraph`.

## Telepítés

1. Csomagold ki a skill-mappába (pl. `~/.claude/skills/callgraph/`).
2. Futtasd: `bash install.sh` — beállítja a jogosultságokat és ellenőrzi a függőségeket.
3. Regisztrálj egy projektet: `bash projects.sh add myapp /abszolút/út/a/projekthez`.
4. Próba: `bash callgraph.sh --project myapp app/Models/User.php`.

- A `bin/<os>-<arch>/callgraph` a platformhoz illő előre fordított bináris (pl. `darwin-arm64`, `linux-x86_64`).
  Más platformra forrásból kell fordítani.
- macOS: a csomag Apple Silicon (`darwin-arm64`) és Intel (`darwin-x86_64`) binárist is tartalmaz (ad-hoc aláírva); az `install.sh` leveszi a letöltés karantén-jelzőjét.
- Linux: `linux-aarch64` és `linux-x86_64` statikus (musl) bináris — bármely disztribúción fut, külső függőség nélkül (a burokhoz `bash` és `jq` kell).
- Forrásból (Rust 1.85+): `cargo build --release`, majd a `target/release/callgraph` másolata a
  `bin/<os>-<arch>/callgraph` helyre (`<os>` = `uname -s` kisbetűvel, `<arch>` = `uname -m`).
- Windows: Git Bash kell (a `uname` `MINGW64_NT-…` értékét a burok `windows`-ra normalizálja); a csomag `bin/windows-x86_64/callgraph.exe` binárist tartalmaz (mingw-w64 cross-build, `tools/build-windows.sh`).
- Külső függőség nincs (se PHP, se Node): az elemzést az `callgraph` végzi; a burokhoz `bash` és `jq` kell.

## Környezeti változók

| Változó | Hatás |
|---|---|
| `CALLGRAPH_BIN` | az `callgraph` bináris útja (alapból `bin/<os>-<arch>/callgraph`, majd `bin/callgraph`) |
| `CALLGRAPH_REGISTRY` | a projekt-alias fájl (alapból a skill `projects.json`-ja) |
| `CALLGRAPH_OUT_LIMIT` | a kimenet-korlát sorszáma (alapból 400) |
| `PHP_AST_HOME` | a hibaüzenetekben mutatott skill-út, ha a skill máshová van linkelve |

Az érték elöl álló argumentumként is megadható: `bash callgraph.sh CALLGRAPH_OUT_LIMIT=100 …`.

## Közvetlen `callgraph`-parancsok

```bash
callgraph <fájl|mappa>...                    # elemzés → fájlonként egy JSONL-sor (jq-val szűrd)
callgraph show --project <gyökér> [kapcsolók] <fájl|mappa>[::metódus]   # amit a callgraph.sh hív
callgraph callers <minta> --depth N          # a hívók hívói N szintig
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
