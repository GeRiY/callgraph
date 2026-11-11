> Go-térkép: structok, interfészek, receiveres metódusok, generikusok, több visszatérési értékű szignatúrák, kategorizált hívások, Gin / Echo / Chi / gRPC / GORM felismerés.

Source: https://callgraph.eu/hu/go/

# Go call graph AI agenteknek

A callgraph a [tree-sitter-go](https://github.com/tree-sitter/tree-sitter-go)-val elemzi a `.go` fájlokat, és megmutatja az agentnek a structokat, interfészeket, függvényeket és metódusokat sortartománnyal, a szignatúrákat és minden hívást kategóriánként.

[Telepítés](https://callgraph.eu/hu/#install) [Használat MCP-n át](https://callgraph.eu/hu/mcp/)

## Mit nyer ki a callgraph a Go-kódból

- Minden fájl kap egy `<fájl>.go` `module` containert a csomag-szintű függvényeivel és konstansaival; a `package` fölötti komment a doksija, a csomag neve namespace-ként látszik.

- A `struct` osztályként: a mezők property-k, a beágyazott típusok (`Base`, `*Logger`) az `extends`-ben. Az `interface` absztrakt metódusokkal, beágyazott interfészekkel és típushalmazokkal.

- Definiált típusok (`type Level int`, `type Handler func()`) és aliasok (`type A = B`). A generikus típusok mutatják a típusparamétereiket (`[T any]`).

- A receiveres metódus a receiver típusához tartozik, pointerrel vagy anélkül. Ha a típust a csomag egy másik fájlja deklarálja, a fájlbeli metódusokat egy `extension` container fogja össze.

- Több visszatérési értékű szignatúrák: `Find(id int): (*User, error)` (`--signatures`, `--format api`). A nagybetűs név publikus (exportált), a kisbetűs privát.

- Csomag-szintű `const` és `var`, importok (alias, blank `_` és dot `.` is), a deklaráció fölötti doc-komment, string-literálok, és szintaktikai hibák figyelmeztetéssel.

- A több `init` függvény `init@L<sor>` néven látszik; a függvény-literál nem külön elem, a hívásai a külső függvényhez tartoznak.

A Gin-t, Echo-t, Fibert, Chi-t, gorilla/mux-ot, gRPC-t, GORM-ot vagy cobrát importáló fájl az adott keretrendszerrel van jelölve. A `vendor` és a `testdata` alapból kimarad; a `_test.go` fájlok a [locate](https://callgraph.eu/hu/locate/)-ben tesztnek számítanak.

## Hívás-kategóriák

| Kategória | Mikor |
|---|---|
| `internal_helper` | ugyanabban a fájlban definiált függvény, vagy `r.f()` a receiveren, fájlbeli metódusra |
| `instantiation` | struct-literál, pl. `Repo{…}` vagy `&pkg.T{…}`, vagy fájlbeli típusra konverzió |
| `static_call` | `Típus.Metódus()` (method expression) fájlbeli típuson |
| `native_go` | beépített függvények, stdlib-csomagok és gyakori metódusok: `make`, `fmt.Println`, `errors.New`, `Close`, `Lock`… |
| `external_call` | importált nem-stdlib csomag, ismeretlen fogadó, a receiveren ismeretlen metódus |
| `global_helper` | ismeretlen puszta név, valószínűleg a csomag másik fájljában definiált |

A `go` és a `defer` mögötti hívások ugyanúgy bekerülnek, mint a többi.

## Parancsok

```
CG=~/.claude/skills/callgraph/callgraph.sh
bash $CG --project myapp <út/a>/cart.go                  # outline: structok, interfészek, függvények, hívások
bash $CG --project myapp <út/a>/cart.go --signatures     # a szignatúra minden metódus-soron
bash $CG --project myapp <út/a>/cart.go::Add --body      # csak az a metódus, sorszámozva
bash $CG --project myapp <út/a>/cart.go --format api     # csak az exportált felület
```

Az outline kimenete egy kis fájlra

```
FILE  shop/cart.go  NS:shop

module cart.go  L1-31
  constants (1):
    TaxRate = 0.27
  __module__()  L8-8
  helper(n int): int  L31-31

class Cart  L11-14
  properties (2):
    []string $Items
    int $total
  Add(item string, p Pricer): error  L21-29
    external_call     p.Price
    internal_helper   helper
    native_go         errors.New, fmt.Println

interface Pricer  L16-18
  Price(item string): (int, error)  L17-17 [abstract]
```

## Ki hívja ezt a Go-függvényt?

```
bash $CG --project myapp <mappa> --grep '(^|::|\.|->)helper$'      # a név minden hívása
bash $CG --project myapp <mappa> --grep '(^|::|\.|->)helper$' --depth 2
```

Gón a callgraph nem old fel típusokat, ezért név szerint keress: a minta a `helper` minden hívását megtalálja, bármi a fogadó. Minden találat a hívás saját forrássorát is mutatja.

## Korlátok

Ez a Go-támogatás első fázisa.

- Nincs típus-feloldás: az `r.repo.Find()` fogadója `r` marad, a belső importot nem oldja fel fájlra, és a generikus függvény típusparaméterei nem látszanak a szignatúrában.

- A beágyazott structból örökölt (promótált) metódusokat és az interfész-megvalósításokat nem követi.

- Még nincsenek keretrendszer-nézetek (Gin / Echo / Chi route-ok, gRPC szolgáltatások, cobra parancsok), sem `--format flow` ágak; a `//go:build` tageket nem értékeli ki.

- A függvényen belüli deklarációk nincsenek listázva.

## Kapcsolódó oldalak

[

### locate

Rangsorolt fájlok a feladat kezdéséhez.

](https://callgraph.eu/hu/locate/) [

### Projekt-profil

Hogyan épül fel egy ismeretlen projekt, kb. 15 sorban.

](https://callgraph.eu/hu/profile/) [

### MCP-szerver

Ugyanezek a lekérdezések bármely MCP-kliensben.

](https://callgraph.eu/hu/mcp/) [

### Python

Osztályok, dekorátorok és szignatúrák.

](https://callgraph.eu/hu/python/)
