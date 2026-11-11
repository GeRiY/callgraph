# LANG-PYTHON — a Python frontend mezőtérképe (1. fázis)

Modul: `src/lang/python/` (`PythonFrontend`, `LanguageRole::Frontend`, azonosító `python`, kiterjesztés `.py`, `.pyi`).
Parser: `tree-sitter = "=0.27.1"` + `tree-sitter-python = "=0.25.0"` (a `tree-sitter-language`-en át illeszkedik). Adapter:
`parser.rs` (`PythonParser` trait + `TreeSitterPython`); tree-sitter-típus csak a `parser.rs` és a `visitor/` alatt
jelenik meg. A hívás-kategorizálás parser-független (`categorize.rs`, `core::Chain`), az ismert nevek adatfájlja
`known.rs` (beépített függvények / kivételek, stdlib-modulok, keretrendszerek). Tesztek: `src/lang/python/**`,
`tests/lang_python.rs`. Alapból kimarad: `__pycache__`, `venv`, `.venv`, `site-packages` (a `*.pyc` nem regisztrált).

## Fájl-szintű keretek
- **framework**: `django` / `flask` / `fastapi`, ha a fájl importálja (`import flask`, `from django.db import models`),
  különben `python`. **namespace**, **module**, **sfc**: nincs.
- **imports**: `import a.b` → `{"a.b": "a.b"}`; `import a.b as c` → `{"c": "a.b"}`; `from a import b as c` →
  `{"c": "a.b"}`; relatív: `from . import x` → `{"x": ".x"}`, `from .m import *` → `{"*": ".m.*"}`. A függvényen belüli
  és `try:` / `if:` alatti importok is felkerülnek. `resolve_module` mindig `None` (a csomag-útvonal a gyökértől függ).
- **Szintaktikai hiba**: a tree-sitter hibatűrő; az első `ERROR` / hiányzó token sora figyelmeztetés, az összes
  (≤ `MAX_PARSE_ERRORS`) a gráf `parse_errors`-ába kerül, a hiba előtti és utáni elemek megmaradnak.

## containers
- Mindig van egy `module` container (`<fájlnév>.py`, `doc` = modul-docstring), a modul-szintű függvények és konstansok
  ennek tagjai. Osztály: `type: class`, `extends` = ősök (a `metaclass=` nélkül), `abstract` ha az ős `ABC`,
  `traits` = dekorátorok forrása (`@dataclass(frozen=True)`; az outline `uses`-ként írja). Beágyazott osztály neve
  `Külső.Belső`. A sortartomány a dekorátor első sorától indul.

## methods
- `def` / `async def`; `kind`: `function` (modul), `method`, `staticmethod`, `classmethod`, `property` (`@property`,
  `@cached_property`, `@x.setter`); `async` esetén `async ` előtag. `static`: staticmethod és classmethod; `abstract`:
  `@abstractmethod`. `visibility`: vezető `_` (nem dunder) → `private`, különben `public`.
- `params`: a paraméterek forrása alapértékkel és annotációval (`b: str = "x"`, `*args`, `**kw`); a metódus `self` /
  `cls` paramétere kimarad. `returnType`: a `->` utáni annotáció. A közös szignatúra-formázó így `name(a: int): bool`
  alakot ír (`--signatures`, `--format api`). `doc`: a törzs első string-literálja (≤300 kar.).
- Név-ütközés (`@x.setter`, túlterhelés): `név@L<sor>`. Beágyazott függvény / lambda nem külön elem, hívásai a külső
  függvényhez tartoznak. A modul-szintű utasítások (pl. `if __name__ == "__main__":`) hívásai a `__module__`
  pszeudo-metódus alatt.

## constants / properties
- Modulban `NAGYBETŰS = …` és minden típusozott értékadás (`X: int = 1`) és az `__all__` konstans (`kind: const`, rövid
  érték). Osztályban `NAGYBETŰS` konstans; a többi (`name: str`, `table = "users"`) property (típus: annotáció, vagy
  literálból / `Foo()`-ból következtetve).

## calls
`kind`: `method_call` / `new` (nagybetűs puszta név) / `func_call`; `target`: `self.repo.save`, `os.path.join`, `Foo`.
Kategóriák: `internal_helper` (fájlbeli függvény, `self.x()` / `cls.x()` fájlbeli metódusra), `instantiation`,
`static_call` (`Osztály.metódus()` fájlbeli osztályon), `native_python` (beépített, stdlib-modul, gyakori beépített
metódus), `external_call` (importált nem-stdlib, ős-hívás `super().x()`, ismeretlen fogadó), `global_helper` (ismeretlen
puszta név). A `self.attr.method()` fogadója `on: "self"` marad (nincs típus-feloldás).

## strings
String-literálok (f-stringnél a statikus részek, a kifejezés `{…}`); a docstring nem.

## Dinamikus hívás-jelölés (`dynamic_call`)
`getattr(obj, név)(…)` nem sztring-literál névvel (a jelölés célja `getattr(obj, név)`), `globals()[név](…)` /
`locals()[név](…)`, `importlib.import_module(név)` / `__import__(név)` nem literál argumentummal, `eval(…)` / `exec(…)`.
`type: "dynamic"`, `resolved: None`; a közönséges hívás (`eval`, `import_module`, a belső `getattr`) megmarad mellette.
Nem hívó-él: a `--grep` / `--depth` / `locate` nem látja. Lásd `src/lang/python/visitor/body.rs` (`dynamic_expr`).

## 2. fázis (nincs kész)
Framework-nézetek (Django `urls`/modellek, Flask/FastAPI route-ok), vezérlési ágak (`if`/`match`/`try`), kommentek
(`TODO`), `--body` tömör nézet és `--const` re-export (`from x import NAME`, `__all__`) a forrásszövegből, `self.x = …`
attribútumok, típus-feloldás (`self.attr` típusa), profil-szerepek (`models.py`, `views.py`, `tests/`, `migrations/`)
és `requirements.txt` / `pyproject.toml` alapú framework-felismerés, relatív import fájlútra oldása, `.pyi` stub-párok.
