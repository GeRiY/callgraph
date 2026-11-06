> A callgraph helyi MCP-szerverként: beállítás Claude Code-hoz, Claude Desktophoz, Cursorhoz és VS Code-hoz, --auto-update, a tíz eszköz, hibakeresés.

Source: https://callgraph.eu/hu/mcp/

# A callgraph MCP-szerver beállítása

A callgraph [Model Context Protocol](https://modelcontextprotocol.io)-szerver is, stdio-n: `callgraph mcp`. A Claude Code, a Claude Desktop, a Cursor, a VS Code vagy bármely más MCP-kliens ugyanazokat a lekérdezéseket típusos eszközként hívja — ugyanazzal a tömör szöveggel és kimeneti korláttal, mint a parancssor.

[Válaszd ki a kliensed](https://callgraph.eu/hu/mcp/#clients) [A tíz eszköz](https://callgraph.eu/hu/mcp/#tools) [Előbb a telepítés](https://callgraph.eu/hu/#install)

## Mielőtt belekezdesz

Az MCP-szerver ugyanaz a bináris, amit a skill is használ, ezért előbb [telepítsd a callgraph-csomagot](https://callgraph.eu/hu/#install). A bináris a skill-csomagban van: `~/.claude/skills/callgraph/bin/<os>-<arch>/callgraph` (mappák: `darwin-arm64`, `darwin-x86_64`, `linux-x86_64`, `linux-aarch64`; Windows: `bin\windows-x86_64\callgraph.exe`).

A JSON-beállításokba **abszolút út** kell (a `~` ott nem bővül). macOS-en / Linuxon ez kiírja:

```
echo ~/.claude/skills/callgraph/bin/$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)/callgraph
```

Nem minden kliens a projektmappában indítja a szervert, ezért egyszer regisztráld a projekteket aliasként:

```
bash ~/.claude/skills/callgraph/projects.sh add shop /abszolút/út/a/shophoz
```

## A kliens beállítása

Minden példa az `--auto-update`-et használja (ajánlott, lásd [frissítés](https://callgraph.eu/hu/mcp/#updates)). A `/Users/te/…/darwin-arm64/callgraph` helyére a fent kiírt út kerül.

### Claude Code

**A telepítővel:** ha a `claude` parancs a PATH-on van, a `bash install.sh` rákérdez, hogy regisztrálja-e az MCP-szervert; a `bash install.sh --mcp` kérdés nélkül megteszi, a `--no-mcp` kihagyja. Újrafuttatva nem vesz fel duplán.

**Kézzel**, macOS / Linux (a géphez illő binárist választja; a `-s user` miatt minden projektben elérhető):

```
claude mcp add -s user callgraph -- \
  ~/.claude/skills/callgraph/bin/$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)/callgraph mcp --auto-update
```

Windows (PowerShell):

```
claude mcp add -s user callgraph -- "$env:USERPROFILE\.claude\skills\callgraph\bin\windows-x86_64\callgraph.exe" mcp --auto-update
```

**Ellenőrzés:** a `claude mcp get callgraph` kiírja a regisztrált parancsot. Indítsd újra a Claude Code-ot; ezután elérhetők a callgraph eszközei. Eltávolítás: `claude mcp remove -s user callgraph`. Claude Code-ban a skill is működik — a kettő megfér egymás mellett.

### Claude Desktop

Ezt írd a `claude_desktop_config.json`-ba:

```
{
  "mcpServers": {
    "callgraph": {
      "command": "/Users/te/.claude/skills/callgraph/bin/darwin-arm64/callgraph",
      "args": ["mcp", "--auto-update"]
    }
  }
}
```

A Claude Desktop nem a projektmappában indítja a szervert: kérdéskor nevezd meg a regisztrált aliast (például `shop`), így az eszközök megkapják a `project` paramétert.

**Ellenőrzés:** indítsd újra a Claude Desktopot, és kérd meg, hogy sorolja fel a callgraph eszközeit, vagy hívja meg a `projects` eszközt — ez a regisztrált aliasokat adja vissza.

### Cursor

Ezt írd a `.cursor/mcp.json`-ba:

```
{
  "mcpServers": {
    "callgraph": {
      "command": "/Users/te/.claude/skills/callgraph/bin/darwin-arm64/callgraph",
      "args": ["mcp", "--auto-update"]
    }
  }
}
```

**Ellenőrzés:** indítsd újra a Cursort, és kérd meg, hogy sorolja fel a callgraph eszközeit.

### VS Code

Ezt írd a `.vscode/mcp.json`-ba:

```
{
  "servers": {
    "callgraph": {
      "type": "stdio",
      "command": "/Users/te/.claude/skills/callgraph/bin/darwin-arm64/callgraph",
      "args": ["mcp", "--auto-update"],
      "cwd": "${workspaceFolder}"
    }
  }
}
```

**Ellenőrzés:** indítsd újra a VS Code-ot, és kérd meg az agentet, hogy sorolja fel a callgraph eszközeit.

### Windows-utak a JSON-ban

Windowson a bináris a `bin\windows-x86_64\` alatti `callgraph.exe`. A JSON-ban minden `\` dupla; a `te` helyére a Windows-felhasználóneved kerül:

```
{
  "mcpServers": {
    "callgraph": {
      "command": "C:\\Users\\te\\.claude\\skills\\callgraph\\bin\\windows-x86_64\\callgraph.exe",
      "args": ["mcp", "--auto-update"]
    }
  }
}
```

VS Code-ban ugyanezt a `command` értéket írd a fenti `servers` bejegyzésbe.

## A tíz eszköz

Az utak a projektgyökérhez relatívak: egy regisztrált alias (`project`) vagy a szerver munkakönyvtára. Szokásos menet: `locate` → `outline` → `method_body` → `find_callers`.

| Eszköz | Mit ad | CLI-megfelelő |
|---|---|---|
| [`locate`](https://callgraph.eu/hu/locate/) | Honnan induljak: rangsorolt fájlok a feladat neveiből és fogalmaiból, mindegyik indoklással (alapból 5, `all: true`: minden találat). | `locate <kifejezés>… [--all]` |
| [`project_profile`](https://callgraph.eu/hu/profile/) | Architektúra, rétegek mappával és fájlszámmal, minták, a rétegek közti hívásirány, ismeretlen mappák. | `profile [--full]` |
| `outline` | Fájl vagy mappa szerkezete: osztályok / komponensek, metódusok sortartománnyal, hívások kategóriánként; `format`: `api`, `flow`, `summary`, `hotspots`, `stats`. | `<út>[::m] --format <f>` |
| `method_body` | Egy vagy több metódus sorszámozott forrása, alapból tömören; a `doc: true` a docblockot is hozza, a `raw: true` a pontos szöveget adja. | `::m --body [--doc\|--raw]` |
| `find_callers` | Ki hívja; a `depth` 2–5 hatáselemzéshez, a `guess: true` a csak név szerinti egyezésekhez. | `--grep <minta> --depth N` |
| `search` | Hol van egy string-literál vagy konstans (`kind`: `strings` \| `constants`). | `--strings` / `--const` |
| `framework_view` | Egy mappa `routes`, `middleware`, `validation`, `jobs`, `events`, `providers`, `magic`, `di`, `views` nézete. | `--format <nézet>` |
| `notes` | A fájlokhoz és metódusokhoz írt jegyzetek (csak olvas; jegyzetet írni csak a CLI-vel lehet). | `--notes <minta>` |
| `projects` | A regisztrált aliasok és a gyökereik. | `projects.sh list` |
| `guide` | A teljes skill-útmutató a beállított nyelven. | `SKILL.md` |

Opcionális réteg-paraméterek (alapból a kimenet nem változik): a `layer` az `outline`, a `search` és a `find_callers` eszközön csak az adott réteg fájljait hagyja meg; a `by_layer` a `find_callers` hívóit rétegek szerint rendezi. Lásd: [projekt-profil](https://callgraph.eu/hu/profile/).

Kapcsolódáskor a szerver rövid használati útmutatót küld a kliensnek a beállított nyelven (melyik eszköz mire való, hogyan keresd a hívókat, hogyan olvasd a kimenetet); a Claude Code ezt az agent utasításai közé teszi. Ha a szerver projektmappában indul, a végére egy tömör projekt-profil is kerül.

## Mi fut helyben

### A saját gépeden

A kliens indítja a szervert, és stdin/stdout-on beszélgetnek. A lekérdezések nem használnak hálózatot: a kódod nem hagyja el a gépet.

### Csak olvas

A forrásfájlokat soha nem írja. Egyetlen mellékhatása a CLI-vel közös projekt-térkép a projekt `.callgraph/` mappájában.

### A projekten belül marad

Minden út a projektgyökérhez képest oldódik fel; a gyökérből kilépő utat (`..`, kívül eső abszolút út, kifelé mutató symlink) elutasítja.

### Magától naprakész

Futás közben figyeli a betöltött projekteket, és a változott fájlokat röviddel a mentés után frissíti. Kikapcsolás: `CALLGRAPH_WATCH=0`.

Az egyetlen hálózati hívás a lenti frissítés-ellenőrzés: naponta legfeljebb egyszer megnézi a legutóbbi GitHub-kiadást (és az `--auto-update` mellett letölti).

## Frissítés: `--auto-update`

- **Az `--auto-update`-tel (ajánlott):** induláskor, naponta legfeljebb egyszer megnézi a legutóbbi kiadást. Ha újabb van, letölti, a kiadás `SHA256SUMS` fájlja szerint ellenőrzi (eltérő vagy hiányzó összegnél semmi nem települ), pontosan úgy telepíti, mint a `callgraph.sh update` (a projektek és a beállítások megmaradnak), és újraindul — a kliens már az új verzióval beszél.

- Bármilyen hibánál (nincs hálózat, rate limit, eltérő ellenőrzőösszeg, nincs írási jog) egy sort ír a stderr-re — `callgraph: auto-update skipped, continuing with …`, ezt a kliensek naplózzák —, és a jelenlegi verzióval indul.

- **Kapcsoló nélkül:** a háttérben ellenőriz, és a munkamenet első válasza végére egy sort tesz: `ℹ callgraph X is available (installed: Y) — update: …`.

- Csak a callgraph saját mappájába ír, a projektjeidbe soha.

- **Minden frissítés-ellenőrzés kikapcsolása:** `CALLGRAPH_UPDATE_CHECK=0` a szerver környezetében (a kliens-konfig `env` mezőjében): ekkor semmilyen hálózati kérés nincs. CI-ben magától is ki van kapcsolva.

## Hibakeresés

### A kliens nem tudja elindítani a szervert

A JSON-beállításokba a bináris abszolút útja kell; a `~` ott nem bővül. Az utat a [Mielőtt belekezdesz](https://callgraph.eu/hu/mcp/#before) részben lévő `echo` parancs írja ki.

### Nem találja az utakat, vagy rossz mappát használ

Egyes kliensek (például a Claude Desktop) nem a projektmappában indulnak. Regisztráld a projektet a `projects.sh add <alias> <út>` paranccsal, és add meg az aliast `project` paraméterként; a `projects` eszköz listázza őket.

### Egy eszköz hibát ad

A hibák eszköz-hibaként jönnek vissza a beállított nyelven; a szerver nem áll le. Az útban nem lehet `::` — a metódus külön paraméterbe kerül.

### A válasz rövidebb a vártnál

A kimenet 400 sornál korlátos (a mappás outline summary-ra, az egyfájlos metódus-indexre vált). Szűkítsd a `path`, `method` vagy `pattern` értékét; a `full: true` csak indokolt esetben kell.

### Nem indul a fájlfigyelés

Ha a figyelés nem lehetséges (például elfogyott az inotify-keret), a szerver egyszer szól a stderr-en, és kérésenként ellenőrzi a változott fájlokat.

### Kézzel szeretném kipróbálni a szervert

Grafikus felülettel: `npx @modelcontextprotocol/inspector <út-a-binárishoz> mcp`.

## Kapcsolódó oldalak

[

### Telepítés

Egysoros telepítés macOS-re / Linuxra, és Windowsra.

](https://callgraph.eu/hu/#install) [

### locate

A feladat első hívása: honnan induljak.

](https://callgraph.eu/hu/locate/) [

### Projekt-profil

Hogyan épül fel egy ismeretlen projekt.

](https://callgraph.eu/hu/profile/) [

### Változásnapló

Mi változott az egyes kiadásokban.

](https://callgraph.eu/hu/changelog/)
