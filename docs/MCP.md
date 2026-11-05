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
| `method_body` | `path`, `method` (egy név stringként vagy névtömb), `project?`, `doc?`, `raw?`, `full?` | a metódus(ok) forrása sorszámozva, tömören (docblock, üres és megjegyzés-sorok nélkül); `doc: true`: docblockkal, `raw: true`: szűretlenül | `show <út>::m --body [--doc\|--raw]` |
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

- **Csak olvas.** Forrásfájlt nem ír; egyetlen mellékhatás a CLI-vel közös elemzési cache és projekt-gráf a projekt
  `.callgraph/` mappájában (ha írható). A jegyzeteket (pilot, `docs/NOTES.md`) is csak olvassa: írni (`note add` / `rm`)
  csak a CLI-vel lehet, MCP-n nincs író eszköz.
- **Projekt-gráf a memóriában:** a szerver a betöltött projekt-gráfot (`docs/PROJECT-GRAPH.md`) és a hívó-indexet
  kérésről kérésre megtartja; kérésenként csak a változott fájlok frissülnek, egy közben szerkesztett fájl a következő
  kérésben már látszik.
- **Útvonal-őr:** minden út a projekt-gyökérhez (alias vagy munkakönyvtár) képest oldódik fel; a gyökérből kilépő út
  (`..`, gyökéren kívüli abszolút út, kifelé mutató symlink) hibát ad. A `::` az útban nem megengedett (a metódus
  külön paraméter).
- **Nincs hálózat** az MCP-úton (a frissítés-ellenőrzés csak a `callgraph.sh` burokban fut).
- **Hibák:** eszköz-hibaként (`isError: true`) jönnek vissza a beállított nyelvű üzenettel — a szerver nem áll le.

## Kliens-beállítás

A bináris a skill-csomagban: `~/.claude/skills/callgraph/bin/<os>-<arch>/callgraph` (pl. `bin/darwin-arm64/callgraph`,
`bin/linux-x86_64/callgraph`, Windows: `bin\windows-x86_64\callgraph.exe`). A JSON-konfigurációkban **abszolút
utat** adj meg (a `~` ott nem bővül). A projekteket érdemes aliasként regisztrálni, mert nem minden kliens a
projektmappában indítja a szervert.

### Claude Code

```bash
claude mcp add callgraph -- ~/.claude/skills/callgraph/bin/linux-x86_64/callgraph mcp
```

(Claude Code-ban a skill is elérhető; a kettő megfér egymás mellett.)

### Claude Desktop (`claude_desktop_config.json`)

```json
{
  "mcpServers": {
    "callgraph": {
      "command": "/Users/me/.claude/skills/callgraph/bin/darwin-arm64/callgraph",
      "args": ["mcp"]
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
      "args": ["mcp"]
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
      "args": ["mcp"],
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
