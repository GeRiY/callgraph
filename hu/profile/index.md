> A callgraph profile kb. 15 sorban leírja az ismeretlen projektet: architektúra, rétegek és mappák, minták, hívásirány. Hozzá a --layer szűrők.

Source: https://callgraph.eu/hu/profile/

# Projekt-profil: architektúra és rétegek kb. 15 sorban

Ismeretlen projektben a mappák listázása helyett futtasd egyszer a `profile`-t. Megmondja, hogyan épül fel a projekt — architektúra, melyik mappában melyik réteg, tervezési minták és a rétegek közti hívásirány — a mappa- és fájlnevekből, a kód-gráffal megerősítve. Nem találgat.

[Telepítés](https://callgraph.eu/hu/#install) [A `project_profile` MCP-eszköz](https://callgraph.eu/hu/mcp/)

## Futtatás

```
bash ~/.claude/skills/callgraph/callgraph.sh profile --project myapp          # kb. 15 sor
bash ~/.claude/skills/callgraph/callgraph.sh profile --project myapp --full   # mappánként, bizonyosság szerint, a rétegek közti hívás-párok
```

## Mi van a profilban

- **Architektúra** — Laravel / Yii rétegzett MVC, DDD / hexagonális, Vue / React frontend, SwiftUI MVVM vagy Rust web, a `composer.json`, `package.json` vagy `Cargo.toml` frameworkjével.

- **Rétegek** — melyik mappában vannak a controllerek, service-ek, repository-k, modellek, komponensek, store-ok…, a fájlszámmal.

- **Tervezési minták** — Repository, Factory, Strategy, Observer…

- **Hívásirány** — a rétegek a várt irányban hívják-e egymást.

- **Ismeretlen mappák** — az egyértelmű szerep nélküli mappák.

A kód-gráf megerősíti, amit a nevek sugallnak: `extends Controller`, `implements *RepositoryInterface`, `defineStore`, és a rétegek közti hívások iránya. Minden fájl kap egy szerepet bizonyossággal (magas / közepes / alacsony), vagy „ismeretlen” marad.

## A rétegek a lekérdezésekben

A réteg egy név a profilból (`controller`, `service`, `repository`, `model`, `component`, `store`…), nem út — így a mappaneveket nem ismerve is szűkíthetsz. Mind kapcsolóra megy; az alapkimenet nem változik.

```
bash ~/.claude/skills/callgraph/callgraph.sh --project myapp app --layer service                       # csak a service-fájlok (több: service,repository)
bash ~/.claude/skills/callgraph/callgraph.sh --project myapp app --format summary --layers             # summary rétegenként
bash ~/.claude/skills/callgraph/callgraph.sh --project myapp app --grep 'Repo::find' --depth 2 --by-layer # hívók rétegek szerint, a legközelebbi elöl
```

- Csak a `--layer` szűr; a `--layers` és a `--by-layer` csak csoportosít és rendez.

- A `--by-layer` a „ki hívja?” találatait rétegek szerint rendezi, a hívott kódhoz legközelebbivel kezdve.

- MCP-n: a `layer` paraméter az `outline`, a `search` és a `find_callers` eszközön, a `by_layer` a `find_callers`-en.

## Saját mappanevek: `.callgraph-layers`

Semmitmondó vagy nem angol mappanevek? Egy commitolható `.callgraph-layers` fájl a projekt gyökerében hozzárendeli őket, például:

```
service = app/Szolgaltatasok, app/Logic
ignore-role = app/Legacy
```

## AI-klienseknek

A `project_profile` MCP-eszköz ugyanezt a profilt adja (`full: true`: részletesen). Ha az MCP-szerver projektmappában indul (ahol van `composer.json`, `package.json`, `Cargo.toml`, `Package.swift`, `.git` vagy `.callgraph`), kapcsolódáskor egy tömör profil is megy a kliensnek. [Az MCP-szerver beállítása](https://callgraph.eu/hu/mcp/).

## Kapcsolódó oldalak

[

### locate

Utána keresd meg a feladat fájljait.

](https://callgraph.eu/hu/locate/) [

### PHP, Laravel és Yii

Rétegzett MVC projektek.

](https://callgraph.eu/hu/php/) [

### Swift / SwiftUI

SwiftUI MVVM projektek.

](https://callgraph.eu/hu/swift/) [

### Újdonság a v0.5.0-ban

A projekt-profil a 0.5.0-val érkezett.

](https://callgraph.eu/hu/changelog/#v0.5.0)
