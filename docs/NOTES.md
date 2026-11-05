# Jegyzetek fájlokhoz és metódusokhoz (PILOT)

> **Pilot funkció.** A formátum és a parancsok még változhatnak; visszajelzést a szokásos úton kérünk
> (`feedback.sh`, GitHub-issue).

Ember és agent is írhat egy rövid jegyzetet egy fájlhoz vagy egy metódushoz („lassú: N+1 lekérdezés”, „ne bővítsd,
kivezetendő”). A callgraph a jegyzetet **a forráson kívül** tárolja, a kódtérképpel együtt mutatja, és kereshető.

## Parancsok

```bash
callgraph note add <fájl>[::<metódus>] "<szöveg>" [--project <alias|gyökér>]   # kiírja az új azonosítót
callgraph note list [<fájl|mappa>]                [--project <alias|gyökér>]   # minden jegyzet, vagy egy út alatt
callgraph note rm <id>                            [--project <alias|gyökér>]
callgraph show <mappa|fájl> --notes <minta>       [--project <gyökér>]         # keresés (regex)
```

A skill-burokkal ugyanígy: `bash ~/.claude/skills/callgraph/callgraph.sh note add …` (a `--project` itt is alias),
és `callgraph.sh <mappa> --notes <minta>`.

- **Projekt-gyökér:** mint a `show`-nál — `--project` (előbb regisztrált alias, utána mappa-út), különben a
  munkakönyvtár. A jegyzet útja ehhez a gyökérhez relatív, ezért ugyanazzal a gyökérrel kell hívni, mint a `show`-t.
- **Horgony:** a metódus **neve** (nem sorszám), így a kód szerkesztése után is a helyén marad. `fájl::metódus`
  vagy `fájl::Osztály::metódus`; az `add` ellenőrzi, hogy a fájl és a metódus létezik, és rögzíti a konténert
  (osztály, impl-típus), ha egyértelmű. A szöveg egy sor (a sortörés szóköz lesz); `-`-sal kezdődő szöveg `--` után:
  `note add a.rs -- "-1 = nincs"`.
- **Idő:** minden jegyzet a létrehozás idejét viseli (UTC, másodpercre tárolva), kiírva `2026-11-05 20:27` alakban.

## Megjelenítés

```text
FILE  /projekt/src/Shop.php
  📝 2026-11-05 20:27 · Kivezetendő, ne bővítsd

class Shop  L2-12
  run()  L4-7
    📝 2026-11-05 20:31 · Lassú: N+1 lekérdezés
    function          strlen
```

- **Outline:** a fájl-jegyzetek a `FILE` fejléc alatt, a metódus-jegyzetek a metódus sora alatt (legújabb elöl,
  horgonyonként legfeljebb 3, utána `📝 … +N`; a szöveg kb. 100 karakterre vágva). Az outline-ban nem listázott
  függvények (procedurális fájl, konténer nélküli függvény) jegyzetei a `FILE` fejléc alatt, `név(): ` előtaggal.
- **`--body`:** a metódus fejléce (`== …`) után, a forrás előtt.
- **Lista / keresés:** `note list` → `<id>  <út>::<Osztály>::<metódus>  2026-11-05 20:27  📝 <szöveg>`;
  `show … --notes <minta>` → `<út>[::<metódus>]  2026-11-05 20:27  📝 <szöveg>` (több találatnál az első sor
  `root: /abs/projekt`, utána relatív utak — mint a többi szöveges kimenetnél). A minta regex a szövegre vagy a
  horgonyra, kis- és nagybetű mindegy; az üres minta mindent listáz.
- **Árva jegyzet:** ha a metódus (vagy a fájl) már nincs meg, a jegyzet megmarad, a lista `[orphaned]` / `[árva]`
  jellel mutatja, az outline nem. Törlés: `note rm <id>`.
- Jegyzet nélkül a kimenet bájtra ugyanaz, mint eddig; a gépi formátumok (`json`, `jsonl`, `mermaid`, `dot`) és a
  többi nézet (api, flow, `--grep` …) nem mutat jegyzetet. A jegyzet-sorok a 400 soros kimeneti korlátba beszámítanak.

## Tárolás

`<projekt>/.callgraph/notes.jsonl` — a `.callgraph/` mappa a saját `*`-os `.gitignore`-jával (nem kerül gitbe).
Soronként egy JSON-objektum:

```json
{"v":1,"id":"3f9a0c1e","path":"src/Shop.php","container":"Shop","method":"run","text":"Lassú: N+1 lekérdezés","created_at":"2026-11-05T20:31:07Z"}
```

| Mező | Jelentés |
|---|---|
| `v` | formátum-verzió (most `1`) |
| `id` | rövid, stabil azonosító (8 hexa-jegy) |
| `path` | a fájl a projekt-gyökérhez képest, `/` elválasztóval |
| `container`, `method` | a horgony név szerint (fájl-jegyzetnél hiányzik) |
| `text` | a jegyzet (egy sor) |
| `created_at` | ISO-8601 UTC, másodpercre |

Az írás atomikus (ideiglenes fájl ugyanabban a mappában, majd átnevezés). A nem értelmezhető sorokat (pl. egy
későbbi verzióét) az írás változatlanul megtartja.

## MCP

Az MCP-szerver csak olvas: a `notes` eszköz keres / listáz (`pattern?`, `path?`, `project?`), írni csak a CLI-vel
lehet. Lásd `docs/MCP.md`.

## Ismert korlátok (pilot)

- Két egyidejű `note add` ugyanabban a projektben elveszítheti az egyiket (nincs zárolás; az írás atomikus, de
  olvasás–írás között nincs zár).
- Átnevezett fájl / metódus jegyzete árva lesz; áthelyezni még nem lehet (törlés + új jegyzet).
- Túlterhelt (azonos nevű) metódusok egy konténeren belül nem különböztethetők meg.
