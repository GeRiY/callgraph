> Python-térkép: osztályok ősökkel és dekorátorokkal, függvények és metódusok, szignatúrák, importok, kategorizált hívások, Django / Flask / FastAPI felismerés.

Source: https://callgraph.eu/hu/python/

# Python call graph AI agenteknek

A callgraph a [tree-sitter-python](https://github.com/tree-sitter/tree-sitter-python)-nal elemzi a `.py` és `.pyi` fájlokat, és megmutatja az agentnek az osztályokat, függvényeket és metódusokat sortartománnyal, a szignatúrákat és minden hívást kategóriánként.

[Telepítés](https://callgraph.eu/hu/#install) [Használat MCP-n át](https://callgraph.eu/hu/mcp/)

## Mit nyer ki a callgraph a Python-kódból

- Minden fájl kap egy `<fájl>.py` `module` containert a modul-szintű függvényeivel és konstansaival; a modul-docstring a doksija.

- Osztályok az ősökkel (`extends`) és dekorátorokkal (`uses @dataclass`); a beágyazott osztályok `Külső.Belső` néven; a sortartomány az első dekorátortól indul. `ABC` ős esetén az osztály absztrakt.

- Függvények és metódusok: `def` és `async def`, `@staticmethod`, `@classmethod`, `@property` (a `@cached_property` és a setterek is), `@abstractmethod`. A vezető aláhúzás privátot jelent. A törzs első string-literálja a docstring.

- Szignatúrák alapértékkel és annotációval: `add(n: int): int` (`--signatures`, `--format api`); a `self` és a `cls` kimarad.

- Konstansok: `NAGYBETŰS` nevek, minden annotált értékadás és az `__all__`; az osztály-attribútumok property-ként, a típussal az annotációból vagy az értékből.

- Importok minden alakban, a relatívak is (`from .m import x`), függvényen belül és `try` / `if` alatt is.

- String-literálok (az f-stringek statikus részei) és szintaktikai hibák: a nem parse-olható fájl is megmutat mindent a hibás sor előtt és után, plusz egy figyelmeztetést.

A `django`-t, `flask`-et vagy `fastapi`-t importáló fájl az adott keretrendszerrel van jelölve. A `__pycache__`, `venv`, `.venv` és `site-packages` alapból kimarad.

## Hívás-kategóriák

| Kategória | Mikor |
|---|---|
| `internal_helper` | ugyanabban a fájlban definiált függvény, vagy `self.f()` / `cls.f()` a fájlbeli metódusra |
| `instantiation` | nagybetűs puszta név, `Foo()` |
| `static_call` | `Osztály.metódus()` a fájlban definiált osztályon |
| `native_python` | beépített függvények, stdlib-modulok és gyakori beépített metódusok: `len`, `os.path.join`, `append`… |
| `external_call` | importált nem-stdlib csomag, `super().x()`, vagy olyan fogadó, amit a callgraph nem tud megnevezni |
| `global_helper` | ismeretlen puszta név, valószínűleg `*`-gal importált vagy máshol definiált |

## Parancsok

```
CG=~/.claude/skills/callgraph/callgraph.sh
bash $CG --project myapp <út/a>/cart.py                  # outline: osztályok, függvények, property-k, hívások
bash $CG --project myapp <út/a>/cart.py --signatures     # a szignatúra minden metódus-soron
bash $CG --project myapp <út/a>/cart.py::add --body      # csak az a metódus, sorszámozva
bash $CG --project myapp <út/a>/cart.py --format api     # csak a publikus felület
```

Az outline kimenete egy kis fájlra

```
FILE  pkg/cart.py

module cart.py  L1-11
  constants (1):
    MAX = 3
  __module__()  L2-2
  helper(n)  L10-11

class Cart uses @dataclass  L4-8
  properties (1):
    int $total
  add(n: int): int  L7-8
    internal_helper   helper
```

## Ki hívja ezt a Python-függvényt?

```
bash $CG --project myapp <mappa> --grep '(^|::|\.|->)helper$'      # a név minden hívása
bash $CG --project myapp <mappa> --grep '(^|::|\.|->)helper$' --depth 2
```

Pythonban a callgraph nem old fel típusokat, ezért név szerint keress: a minta a `helper` minden hívását megtalálja, bármi a fogadó. Minden találat a hívás saját forrássorát is mutatja.

## Korlátok

Ez a Python-támogatás első fázisa.

- Nincs típus-feloldás: a `self.repo.save()` fogadója `self` marad, és az importált modult nem oldja fel fájlra.

- A keretrendszer-felismerés csak a fájl importjait nézi; még nincsenek Django URL- / modell-nézetek, sem Flask / FastAPI route-nézetek, és nincsenek `--format flow` ágak.

- A beágyazott függvény és a lambda nem külön elem: a hívásai a külső függvényhez tartoznak.

- A beépített metódusok listája név szerinti: egy saját `append` is `native_python`.

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

### Go

Structok, interfészek és receiveres metódusok.

](https://callgraph.eu/hu/go/)
