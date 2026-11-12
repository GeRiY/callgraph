> Szerkezet, hívás-gráf és hívók AI agenteknek a fájl beolvasása nélkül. PHP, JS/TS, Vue, Rust, Swift, Python, Go. Ingyenes, MIT, egy bináris: CLI vagy MCP-szerver.

Source: https://callgraph.eu/hu/

[v0.9.3 — újdonságok →](https://callgraph.eu/hu/changelog/)

# Értsd meg a kódot, olvasás nélkül.

A **callgraph** token-hatékony kód-térkép AI kódoló agenteknek. Megmutatja a szerkezetet, a kategorizált hívás-gráfot, egy-egy metódus törzsét, hogy *„ki hívja?”*, és a hatáselemzést — pár tucat sorban, a teljes fájlok helyett.

[Telepítés egy sorral](https://callgraph.eu/hu/#install) [Letöltés: v0.9.3](https://github.com/GeRiY/callgraph/releases/latest) [Megnézem a GitHubon](https://github.com/GeRiY/callgraph)

```
$ callgraph.sh --project shop app/Models/Order.php
class Order extends Model  L7-23
  constants (1):
    STATUS_OPEN = 'open'
  saveTranslations()  L13-20
    external_call     create, info
    global_helper     logger
    internal_helper   translations
  translations()  L22-22
    orm_relation      hasMany

$ callgraph.sh app --grep saveTranslations --depth 2
== 1. szint — közvetlenül hívja: /saveTranslations/
  app/Models/Order.php  Caller::run()  L25-25
== 2. szint — az előző szintet hívja
  (nincs további hívó)
```

## Miért szeretik az agentek (és az emberek)

Minden válasz a szintaxisfából jön, ezért pontos, tömör és olcsón olvasható.

### [Honnan induljon a feladat?](https://callgraph.eu/hu/locate/)

A `locate` egy hívásban megkapja a feladat neveit és fogalmait, és felsorolja azt a pár fájlt, amit érdemes először megnyitni, a legjobbal kezdve, indoklással.

### [Projekt-profil](https://callgraph.eu/hu/profile/)

A `profile` megmondja az agentnek, hogyan épül fel egy ismeretlen projekt: architektúra, melyik mappában melyik réteg, tervezési minták, és a rétegek közti hívásirány.

### Kevesebb token, gyorsabb válasz

Egy outline pár tucat sor. Az agent előbb a térképet olvassa, utána csak a szükséges sorokat.

### Kategorizált hívás-gráf

Minden hívás címkét kap — ORM-reláció, modell, framework, natív, belső segéd, SwiftUI view vagy modifier —, így a szándék első ránézésre látszik.

### „Ki hívja?”

A `--grep` megtalál minden hívót a projektben; a `--depth N` a hívók hívóit is bejárja hatáselemzéshez.

### Célzott olvasás

A `fájl::metódus --body` csak azt a metódust írja ki, sorszámozva. A `--lines 30-60` bármely sávot.

### Framework-nézetek

Laravel / Yii ismeret: routes, middleware, validation, jobs, events, providers, DI, views — mind egy-egy parancs.

### Egyetlen bináris, magától frissül

Nem kell PHP, Node vagy Rust. macOS, Linux és Windows. Gyors, cache-elt, inkrementális; a `callgraph.sh update` letölti a legújabb kiadást, a beállítások megmaradnak.

### Angolul vagy magyarul

Minden kimenet és súgó alapból angol — magyarra: `callgraph.sh set-lang hu`.

## Mikor használd — és mikor ne

A callgraph és a sima szövegkeresés más-más feladatra való. Mindegyiket arra használd, amiben a legjobb.

| A kérdés | A legjobb eszköz |
|---|---|
| Pontos string, naplóüzenet vagy konfig-érték | grep — a sima szövegkeresés ebben a legjobb |
| Honnan induljak ezzel a feladattal? | [`locate`](https://callgraph.eu/hu/locate/) — rangsorolt fájlok a feladat neveiből és fogalmaiból |
| Hogyan épül fel ez az ismeretlen projekt? | [`profile`](https://callgraph.eu/hu/profile/) — architektúra, rétegek, minták |
| Definíciók, szerkezet, hívók, egy változtatás hatása | callgraph — outline, `--body`, `--grep`, `--depth` |

### Mikor *ne* használd

- Egy ismert sorsáv pontos szövege kell: `--lines` vagy célzott olvasás.

- Nem kódfájlok (sima szöveg, adat-dump): az outline keveset ad hozzá.

- Makró- vagy reflexió-nehéz kód: a statikusan fel nem oldható dinamikus hívások csak név szerinti egyezésként (`?`) látszanak a `--guess`-szel, vagy egyáltalán nem.

## Támogatott nyelvek

Minden nyelvnek ugyanaz a kimeneti formája.

| Nyelv | Fájlok | Mit kapsz |
|---|---|---|
| [PHP](https://callgraph.eu/hu/php/) | `.php` | osztályok, traitek, enumok, metódusok, property-k, konstansok, hívások típuskövetéssel, Laravel / Yii felismerés |
| [JavaScript / TypeScript](https://callgraph.eu/hu/typescript/) | `.js .mjs .cjs .jsx .ts .tsx` | modulok, függvények, osztályok, importok alias-feloldással |
| [Vue](https://callgraph.eu/hu/typescript/) | `.vue` | props, emits, setup, template-komponensek és eseménykezelők, computed, watch, hookok |
| [Rust](https://callgraph.eu/hu/rust/) | `.rs` | modulok, structok, enumok, traitek, impl-blokkok, függvények, use-fák, makróhívások, framework-felismerés (axum, actix-web, tokio…) |
| [Swift / SwiftUI](https://callgraph.eu/hu/swift/) | `.swift` | osztályok, structok, protokollok, actorok, extensionök, View-komponensek, property wrapperek, SwiftUI view-k és modifierek |
| [Python](https://callgraph.eu/hu/python/) | `.py .pyi` | osztályok ősökkel és dekorátorokkal, függvények, metódusok, szignatúrák, importok, Django / Flask / FastAPI felismerés |
| [Go](https://callgraph.eu/hu/go/) | `.go` | structok, interfészek, receiveres metódusok, generikusok, több visszatérési értékű szignatúrák, Gin / Echo / Chi / gRPC / GORM felismerés |
| CSS / SCSS | `.css .scss` | szabály-blokkok, mixinek, függvények, változók, importok |
| JSON / Markdown | `.json .md` | kulcs-útvonalak; címsor-szakaszok és linkek |

## Telepítés

A csomagban a Claude Code skill és a géped binárisa van. Csak `bash` kell hozzá.

### macOS / Linux

```
p="$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)" && t="$(mktemp -d)" \
  && curl -fsSL -o "$t/cg.zip" "https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-$p.zip" \
  && mkdir -p ~/.claude/skills && unzip -qo "$t/cg.zip" -d ~/.claude/skills \
  && bash ~/.claude/skills/callgraph/install.sh && rm -rf "$t"
```

### Windows

Töltsd le a [callgraph-windows-x86_64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-windows-x86_64.zip)-et, csomagold ki, a `callgraph` mappát másold a `%USERPROFILE%\.claude\skills\` alá, majd Git Bashben:

```
bash ~/.claude/skills/callgraph/install.sh
```

Utána egyszer regisztráld a projektet, és indítsd újra a Claude Code-ot:

```
bash ~/.claude/skills/callgraph/projects.sh add shop /abszolút/út/a/shophoz
```

Minden letöltés (macOS Apple Silicon és Intel, Linux x86_64 és ARM64, Windows): [legfrissebb kiadás](https://github.com/GeRiY/callgraph/releases/latest).

## Használat

Az agent magától használja a skillt. Ugyanezek a parancsok bármelyik terminálból működnek.

| Feladat | Parancs |
|---|---|
| Honnan induljak ezzel a feladattal? ([locate](https://callgraph.eu/hu/locate/)) | `callgraph.sh locate <kifejezés>... [--all]` |
| Hogyan épül fel a projekt? ([profile](https://callgraph.eu/hu/profile/)) | `callgraph.sh profile [--full]` |
| Fájl vagy mappa outline-ja | `callgraph.sh <fájl\|mappa>` |
| Csak egy metódus | `callgraph.sh <fájl>::<metódus>` |
| Metódus forrása sorszámozva | `callgraph.sh <fájl>::<metódus> --body` |
| Hol hívják? | `callgraph.sh <mappa> --grep <minta>` |
| Hatáselemzés | `callgraph.sh <mappa> --grep <minta> --depth 3` |
| String / konstans helye | `--strings <minta>` · `--const <minta>` |
| Ágak sortartománnyal | `--format flow` |
| Csak a publikus felület | `--format api` |
| Hol nézz körül elsőként | `--format hotspots` |
| Diagramok | `--format mermaid` · `--format dot` |
| Laravel-nézetek | `--format routes` · `validation` · `events` · `jobs` |

### Frissítés és nyelv

```
callgraph.sh update --check     # van újabb kiadás?
callgraph.sh update             # letöltés és telepítés (a beállítások megmaradnak)
callgraph.sh set-lang hu        # magyar kimenet (vissza: set-lang en)
```

## Bármely MCP-kliensből

A callgraph MCP-szerver is: `callgraph mcp`. A Claude Desktop, a Cursor, a VS Code, a Claude Code vagy bármely más MCP-kliens ugyanazokat a lekérdezéseket típusos eszközként hívja, ugyanazzal a tömör kimenettel.

### 🔌 Helyben fut

A kliens a saját gépeden indítja el, és stdin/stdout-on beszélgetnek. Nincs szerver, a lekérdezések nem használnak hálózatot: a kódod nem hagyja el a gépet. Az egyetlen hálózati hívás a naponta egyszeri frissítés-ellenőrzés a GitHub Release-ek felé.

### 🔒 Csak olvas

A forrásfájlokat soha nem írja, és csak a projekt mappáján belül olvas; a kifelé mutató utakat elutasítja.

### 🧰 Tíz eszköz

`locate`, `project_profile`, `outline`, `method_body`, `find_callers`, `search`, `framework_view`, `projects`, `notes`, valamint `guide` — ugyanazok a lekérdezések, mint a fenti parancsok, és egy rövid útmutató az agentnek.

Válaszd ki a rendszered — a parancsok a telepített skill-csomag binárisát használják. Az `--auto-update`-tel (ajánlott) a szerver induláskor naponta egyszer ellenőriz, telepíti az új kiadást (ellenőrzőösszeggel) és újraindul; nélküle csak jelzi az agentnek, hogy van frissítés. Az ellenőrzés kikapcsolása: `CALLGRAPH_UPDATE_CHECK=0`.

### macOS / Linux

Claude Code (minden projekthez):

```
claude mcp add -s user callgraph -- \
  ~/.claude/skills/callgraph/bin/$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)/callgraph mcp --auto-update
```

A lenti JSON-beállításokhoz a bináris abszolút útja kell — ez kiírja:

```
echo ~/.claude/skills/callgraph/bin/$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)/callgraph
```

Claude Desktop (`claude_desktop_config.json`) · Cursor (`.cursor/mcp.json`):

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

VS Code (`.vscode/mcp.json`):

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

A példa-utat cseréld a fent kiírtra (mappák: `darwin-arm64`, `darwin-x86_64`, `linux-x86_64`, `linux-aarch64`).

### Windows

Claude Code (PowerShell, minden projekthez):

```
claude mcp add -s user callgraph -- "$env:USERPROFILE\.claude\skills\callgraph\bin\windows-x86_64\callgraph.exe" mcp --auto-update
```

Claude Desktop (`claude_desktop_config.json`) · Cursor (`.cursor/mcp.json`):

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

VS Code (`.vscode/mcp.json`):

```
{
  "servers": {
    "callgraph": {
      "type": "stdio",
      "command": "C:\\Users\\te\\.claude\\skills\\callgraph\\bin\\windows-x86_64\\callgraph.exe",
      "args": ["mcp", "--auto-update"],
      "cwd": "${workspaceFolder}"
    }
  }
}
```

A `te` helyére a Windows-felhasználóneved kerül (a JSON-ban minden `\` dupla).

Kliensenkénti beállítás lépésről lépésre, a tíz eszköz és hibakeresés: [MCP-szerver beállítása](https://callgraph.eu/hu/mcp/).

A skill és az MCP-szerver megfér egymás mellett. Ha a kliens nem a projektmappában indul (pl. Claude Desktop), egy regisztrált projekt-aliast használ: `projects.sh add shop /út/a/shophoz`.

## Merre tovább

Útmutatók nyelvenként és funkciónként.

[

### MCP-szerver beállítása

Claude Code, Claude Desktop, Cursor és VS Code beállítás, `--auto-update`, a tíz eszköz.

](https://callgraph.eu/hu/mcp/) [

### PHP, Laravel és Yii

Osztályok, kategorizált hívások, hívók interfészen és traiten át, framework-nézetek.

](https://callgraph.eu/hu/php/) [

### Rust

Structok, traitek, `impl`-blokkok, makróhívások, framework a `Cargo.toml`-ból.

](https://callgraph.eu/hu/rust/) [

### JavaScript, TypeScript és Vue

Importok alias-feloldással, Vue SFC-k, típusos fogadók, route- és i18n-kulcsok.

](https://callgraph.eu/hu/typescript/) [

### Swift és SwiftUI

View-típusok komponensként, property wrapperek, SwiftUI view-k és modifierek.

](https://callgraph.eu/hu/swift/) [

### Python

Osztályok, dekorátorok, szignatúrák, Django / Flask / FastAPI felismerés.

](https://callgraph.eu/hu/python/) [

### Go

Structok, interfészek, receiveres metódusok, több visszatérési érték.

](https://callgraph.eu/hu/go/) [

### locate

Az a pár fájl, amit egy feladatnál először érdemes megnyitni, indoklással.

](https://callgraph.eu/hu/locate/) [

### Projekt-profil

Architektúra, rétegek és a köztük lévő hívásirány, kb. 15 sorban.

](https://callgraph.eu/hu/profile/) [

### Változásnapló

Mit hozott, mit változtatott és mit javított minden kiadás.

](https://callgraph.eu/hu/changelog/)

## Hibajelentés és visszajelzés

Hibát találtál, vagy ötleted van? Minden jelzést elolvasunk. Előbb nézd meg a [meglévő issue-kat](https://github.com/GeRiY/callgraph/issues).

### 🐞 Hiba jelzése

Nyiss egy [hibajelentést](https://github.com/GeRiY/callgraph/issues/new?template=bug_report.yml). Az űrlap végigvezet azon, mi kell nekünk.

### 💡 Ötlet, észrevétel

Hiányzó nyelv vagy framework, jobb kimenet, bármi: [oszd meg az ötleted](https://github.com/GeRiY/callgraph/issues/new?template=feature_request.yml).

### 🔒 Biztonsági hiba

**Ne** nyilvános issue-ban jelezd, hanem [privátban](https://github.com/GeRiY/callgraph/security/advisories/new). Néhány napon belül válaszolunk.

### Mit írj a hibajelentésbe

- a verziót: `callgraph --version`

- a pontos parancsot és a kimenetét

- ha a kimenetben van `[error-id: …]` sor: a `bash ~/.claude/skills/callgraph/feedback.sh show <error-id>` kimenetét — maga a hibaüzenet is a hibajelentő űrlapra mutat

- egy kis, nem bizalmas kódrészletet, amivel előjön, és az operációs rendszert

## Gyakori kérdések

### Mi a callgraph?

Parancssori kód-térkép AI kódoló agenteknek. Elemzi a fájlt vagy mappát, és megmondja, mi van benne, melyik metódus melyik sorokon él, mit hív, ki hívja, és mit érint egy változtatás — pár tucat sorban a teljes fájl helyett.

### Milyen nyelveket támogat?

PHP (Laravel- és Yii-ismerettel), JavaScript, TypeScript, Vue single-file komponensek, Rust, Swift és SwiftUI, Python, Go, CSS, SCSS, JSON, Markdown és HTML. Részletek: [PHP](https://callgraph.eu/hu/php/), [Rust](https://callgraph.eu/hu/rust/), [TypeScript / Vue](https://callgraph.eu/hu/typescript/), [Swift](https://callgraph.eu/hu/swift/), [Python](https://callgraph.eu/hu/python/), [Go](https://callgraph.eu/hu/go/).

### Kell hozzá PHP, Node vagy Rust?

Nem. A callgraph egyetlen statikus bináris, futtatáshoz csak bash kell.

### Működik Claude Code nélkül is?

Igen. Claude Code skillként érkezik, de minden parancs bármilyen terminálból vagy szkriptből használható, és bármely MCP-kliens (Claude Desktop, Cursor, VS Code…) is használhatja a `callgraph mcp`-vel — lásd az [MCP-beállítást](https://callgraph.eu/hu/mcp/).

### Elküldi valahova a kódomat?

Nem. Minden helyben fut; a kódod sehova nem töltődik fel. Az egyetlen hálózati hívás a frissítés-ellenőrzés a GitHub Release-ek felé (legfeljebb naponta egyszer; kikapcsolás: `CALLGRAPH_UPDATE_CHECK=0`).

### Hogyan frissíthető?

A `callgraph.sh update` letölti a legfrissebb kiadást a GitHubról, ellenőrzi a kiadás `SHA256SUMS` fájlja szerint, a projektek és a beállítások megmaradnak. Mi változott: [változásnapló](https://callgraph.eu/hu/changelog/).

### Hol jelezhetek hibát vagy kérhetek új funkciót?

A GitHubon: [hibajelentés](https://github.com/GeRiY/callgraph/issues/new?template=bug_report.yml) vagy [ötlet / észrevétel](https://github.com/GeRiY/callgraph/issues/new?template=feature_request.yml). Biztonsági hibát [privátban](https://github.com/GeRiY/callgraph/security/advisories/new) jelezz.

### Mennyibe kerül?

Semmibe. A callgraph nyílt forrású, MIT licenccel.
