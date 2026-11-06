# MCP-szerver mód (`callgraph mcp`)

A callgraph kétféleképpen használható, a kettő egymás mellett, párhuzamosan is:

1. **Skill / CLI (Claude Code)** — `callgraph.sh …` / `callgraph show …`, a `skill/SKILL.md` útmutatásával. Ez
   változatlan, teljes értékűen támogatott.
2. **MCP-szerver (bármely MCP-kliens)** — `callgraph mcp` a [Model Context Protocol](https://modelcontextprotocol.io)
   szervere stdio-n. Claude Desktop, Cursor, VS Code (Copilot), Claude Code vagy más MCP-kliens ugyanazokat a
   lekérdezéseket eszközként (tool) hívja.

Az MCP-mód a hivatalos Rust SDK-ra (`rmcp`) épül, és a meglévő lekérdező-motort futtatja: a válasz szövege bájtra
ugyanaz a tömör kimenet, amit a `callgraph show` kiír (ez a token-hatékony rész), ugyanazzal a kimeneti korláttal.

## Eszközök

| Eszköz | Paraméterek | Mit ad | CLI-megfelelő |
|---|---|---|---|
| `outline` | `path`, `project?`, `format?` (`outline`\|`api`\|`flow`\|`summary`\|`hotspots`\|`stats`), `method?`, `full?` | fájl / mappa szerkezete; `method`-dal egy metódus hívás-gráfja | `show <út>[::m] --format <f>` |
| `method_body` | `path`, `method` (egy név stringként vagy névtömb), `project?`, `doc?`, `raw?`, `full?` | a metódus(ok) forrása sorszámozva, tömören (docblock és üres sorok nélkül, a törzsbeli megjegyzések maradnak; a kihagyott docblockot a fejléc jelzi); `doc: true`: docblockkal, `raw: true`: szűretlenül | `show <út>::m --body [--doc\|--raw]` |
| `find_callers` | `pattern`, `path?`, `project?`, `depth?` (1–5), `guess?`, `full?` | ki hívja; `depth` > 1: hatáselemzés | `show <út> --grep <minta> --depth N [--guess]` |
| `search` | `kind` (`strings`\|`constants`), `pattern`, `path?`, `project?`, `full?` | string-literál / konstans helye | `show <út> --strings` / `--const` |
| `framework_view` | `view` (`routes`\|`middleware`\|`validation`\|`jobs`\|`events`\|`providers`\|`magic`\|`di`\|`views`), `path`, `project?`, `full?` | framework-nézet | `show <út> --format <nézet>` |
| `notes` (pilot) | `pattern?` (regex a szövegre / horgonyra; üres = mind), `path?`, `project?` | a fájlokhoz / metódusokhoz írt jegyzetek: `út[::metódus]  idő  📝 szöveg`, `[orphaned]`, ha a metódus már nincs | `show <út> --notes <minta>` |
| `projects` | — | a regisztrált aliasok és gyökereik | `projects.sh list` |

- A `path` a projekt-gyökérhez képest értendő; elhagyva (ahol opcionális) a teljes gyökér.
- A `project` egy regisztrált alias (`projects.sh add <alias> <út>`, a `projects.json`-ból); nélküle a gyökér a
  szerver munkakönyvtára.
- **Kimeneti korlát:** mint a CLI-ben — 400 sor fölött a mappás outline `summary`-ra, az egyfájlos metódus-indexre
  vált (a küszöb: `CALLGRAPH_OUT_LIMIT`). A `full: true` a `--full` megfelelője, csak indokolt esetben.
- A nyelv (angol / magyar) a szokásos beállítást követi: `CALLGRAPH_LANG` › `callgraph set-lang` › angol.

## Biztonság

- **Csak olvas.** Forrásfájlt nem ír; egyetlen mellékhatás a CLI-vel közös projekt-gráf a projekt
  `.callgraph/` mappájában (ha írható; a fájlfigyelő is csak ezt frissíti). A jegyzeteket (pilot, `docs/NOTES.md`) is csak olvassa: írni (`note add` / `rm`)
  csak a CLI-vel lehet, MCP-n nincs író eszköz.
- **Projekt-gráf a memóriában:** a szerver a betöltött projekt-gráfot (`docs/PROJECT-GRAPH.md`: manifest, szűkített tények, a már betöltött teljes rekordok) és a hívó-indexet
  kérésről kérésre megtartja; kérésenként csak a változott fájlok frissülnek, egy közben szerkesztett fájl a következő
  kérésben már látszik.
- **Fájlfigyelő:** a szerver a már betöltött projekt-gyökereket figyeli (`notify`: macOS FSEvents, Linux inotify,
  Windows), és a változott fájlokat röviddel (300 ms csend) a változás után a háttérben frissíti a memóriában és a
  `.callgraph/` tárolóban — így a következő kérés nem elemez. A `.callgraph/`, `.git/`, `target/`, `node_modules/`,
  `vendor/` és a bejárás által kihagyott fájlok kimaradnak. A válaszok szövege ettől nem változik. Ha a figyelés nem
  indul (pl. elfogyott az inotify-keret), egyszer szól a stderr-en, és marad a kérésenkénti ellenőrzés. Kikapcsolás:
  `CALLGRAPH_WATCH=0` a szerver környezetében. Részletek: `docs/PROJECT-GRAPH.md`.
- **Útvonal-őr:** minden út a projekt-gyökérhez (alias vagy munkakönyvtár) képest oldódik fel; a gyökérből kilépő út
  (`..`, gyökéren kívüli abszolút út, kifelé mutató symlink) hibát ad. A `::` az útban nem megengedett (a metódus
  külön paraméter).
- **Hálózat csak a frissítéshez:** a lekérdezések soha nem használnak hálózatot; naponta legfeljebb egyszer a
  legutóbbi kiadás lekérdezése (és `--auto-update` mellett a letöltése) — lásd lent: [Frissítés](#frissítés).
- **Hibák:** eszköz-hibaként (`isError: true`) jönnek vissza a beállított nyelvű üzenettel — a szerver nem áll le.

## Frissítés

Az MCP-kliensek (Claude Code, Claude Desktop, Cursor, VS Code) a munkamenet elején indítják a szervert — ugyanazt a
binárist, amit a skill is használ. Ajánlott beállítás: **`"args": ["mcp", "--auto-update"]`**.

- **`--auto-update` (ajánlott, önkéntes):** indításkor, még a kézfogás előtt, naponta legfeljebb egyszer megnézi a
  legutóbbi kiadást (rövid időkorlátokkal: 3 s kapcsolódás, 30 s kérésenként). Ha újabb van, letölti, a kiadás
  `SHA256SUMS`-a szerint **kötelezően** ellenőrzi (eltérő vagy hiányzó összegnél semmi nem települ), telepíti
  pontosan úgy, mint a `callgraph.sh update` (bináris + skill-fájlok; a `projects.json`, `lang`, hibanapló stb.
  marad), majd az új binárissal újraindul ugyanazokkal az argumentumokkal — a kliens már az új verzióval beszél.
  - Unix (macOS, Linux): `exec` — ugyanaz a folyamat (PID) és ugyanaz a stdin/stdout.
  - Windows: a futó `.exe` nem írható felül, ezért a telepítő `callgraph.exe.old`-ra nevezi, és az új az eredeti
    helyére kerül; a régi folyamat ezt gyerekként indítja az örökölt stdin/stdout/stderr-rel, megvárja, és a
    kilépési kódját továbbadja. A `.old` fájlt egy későbbi indítás törli.
  - Az újraindított folyamat a `CALLGRAPH_MCP_REEXEC=1` jelzőt kapja, és kihagyja a frissítési lépést (nem lehet
    hurok).
  - **Bármilyen hibánál** (nincs hálózat, rate limit, eltérő ellenőrzőösszeg, írási jog) egy sor megy a stderr-re
    (`callgraph: auto-update skipped, continuing with …`; a kliensek naplózzák), és a szerver a jelenlegi verzióval
    indul. Aznap nem próbálkozik újra.
- **Kapcsoló nélkül:** a szerver a háttérben (külön szálon, a kéréseket soha nem várakoztatva) naponta legfeljebb
  egyszer megnézi a legutóbbi kiadást; ha újabb van, a munkamenet első (sikeres) eszköz-eredménye végére egy sor kerül:
  `ℹ callgraph X is available (installed: Y) — update: bash <skill>/callgraph.sh update, or add --auto-update to the
  MCP config`. Ugyanarról a verzióról 24 óránként legfeljebb egyszer szól (közösen a `callgraph.sh` értesítésével).

**Mit ír:** csak a callgraph saját telepítési mappájába (a skill-mappa, pl. `~/.claude/skills/callgraph/`): az
`update-check.json` állapotfájlt (utolsó ellenőrzés, legutóbbi verzió, utolsó értesítés / frissítési kísérlet), és
frissítéskor a csomag fájljait. Projektfájlokat soha. **A stdout a protokollé:** ide a frissítés semmit nem ír.

**Kikapcsolás:** `CALLGRAPH_UPDATE_CHECK=0` a szerver környezetében (a kliens-konfig `env` mezőjében) — ekkor
semmilyen hálózati kérés nincs, `--auto-update` mellett sem. CI-ben (`CI` be van állítva) és skill-telepítésen
kívüli binárisnál (pl. `target/release/callgraph`) automatikusan ki van kapcsolva.

## Kliens-beállítás

A bináris a skill-csomagban: `~/.claude/skills/callgraph/bin/<os>-<arch>/callgraph` (pl. `bin/darwin-arm64/callgraph`,
`bin/linux-x86_64/callgraph`, Windows: `bin\windows-x86_64\callgraph.exe`). A JSON-konfigurációkban **abszolút
utat** adj meg (a `~` ott nem bővül). A projekteket érdemes aliasként regisztrálni, mert nem minden kliens a
projektmappában indítja a szervert.

### Claude Code

macOS / Linux (az `uname` alapján a géphez illő binárist választja; `-s user`: minden projektben elérhető):

```bash
claude mcp add -s user callgraph -- \
  ~/.claude/skills/callgraph/bin/$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)/callgraph mcp --auto-update
```

Windows (PowerShell):

```powershell
claude mcp add -s user callgraph -- "$env:USERPROFILE\.claude\skills\callgraph\bin\windows-x86_64\callgraph.exe" mcp --auto-update
```

(A `--auto-update` elhagyható: akkor a szerver csak szól, ha újabb verzió van — lásd [Frissítés](#frissítés).)

A JSON-beállításokhoz kellő abszolút utat ez írja ki (macOS / Linux):

```bash
echo ~/.claude/skills/callgraph/bin/$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)/callgraph
```

(Claude Code-ban a skill is elérhető; a kettő megfér egymás mellett.)

### Claude Desktop (`claude_desktop_config.json`)

```json
{
  "mcpServers": {
    "callgraph": {
      "command": "/Users/me/.claude/skills/callgraph/bin/darwin-arm64/callgraph",
      "args": ["mcp", "--auto-update"]
    }
  }
}
```

A Claude Desktop nem a projektmappában indít: add meg a `project` aliast (lásd a `projects` eszközt).

### Cursor (`.cursor/mcp.json`)

```json
{
  "mcpServers": {
    "callgraph": {
      "command": "/Users/me/.claude/skills/callgraph/bin/darwin-arm64/callgraph",
      "args": ["mcp", "--auto-update"]
    }
  }
}
```

### VS Code (`.vscode/mcp.json`)

```json
{
  "servers": {
    "callgraph": {
      "type": "stdio",
      "command": "/Users/me/.claude/skills/callgraph/bin/darwin-arm64/callgraph",
      "args": ["mcp", "--auto-update"],
      "cwd": "${workspaceFolder}"
    }
  }
}
```

## Kipróbálás kézzel

Egy sor = egy JSON-RPC üzenet a stdin-en:

```bash
{ printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"outline","arguments":{"path":"golden/samples/swift/Todo.swift"}}}'
  sleep 2; } | callgraph mcp      # a sleep: a stdin ne záruljon le a válaszok előtt
```

Grafikus felülettel: `npx @modelcontextprotocol/inspector <út-a-binárishoz> mcp`.
