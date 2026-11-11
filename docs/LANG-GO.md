# LANG-GO — a Go frontend mezőtérképe (1. fázis)

Modul: `src/lang/go/` (`GoFrontend`, `LanguageRole::Frontend`, azonosító `go`, kiterjesztés `.go`).
Parser: `tree-sitter = "=0.27.1"` + `tree-sitter-go = "=0.25.0"` (a `tree-sitter-language`-en át illeszkedik). Adapter:
`parser.rs` (`GoParser` trait + `TreeSitterGo`); tree-sitter-típus csak a `parser.rs` és a `visitor/` alatt jelenik
meg. A hívás-kategorizálás parser-független (`categorize.rs`, `core::Chain`), az ismert nevek adatfájlja `known.rs`
(beépített függvények / típusok, stdlib-gyökércsomagok, keretrendszerek). Tesztek: `src/lang/go/**`,
`tests/lang_go.rs`. Alapból kimarad: `vendor`, `testdata`; a `_test.go` fájlok a `locate`-ben tesztnek számítanak
(`src/locate/rank.rs`).

## Fájl-szintű keretek
- **namespace**: a `package` neve. **framework**: `gin` / `echo` / `fiber` / `chi` / `gorilla-mux` / `grpc` / `gorm` /
  `cobra`, ha a fájl importálja az adott út-előtagot, különben `go`. **module**, **sfc**: nincs.
- **imports**: `import "net/http"` → `{"http": "net/http"}`; `import l "x/log"` → `{"l": "x/log"}`; a csomagnév az út
  utolsó eleme (`/v2` major-verzió és `.v3` végződés nélkül: `gopkg.in/yaml.v3` → `yaml`). A blank (`_`) és a dot (`.`)
  import nem köt nevet: a kulcs maga az út. `resolve_module` mindig `None` (az út a `go.mod` modulútvonalától függ).
- **Szintaktikai hiba**: a tree-sitter hibatűrő; az első `ERROR` / hiányzó token sora figyelmeztetés, az összes
  (≤ `MAX_PARSE_ERRORS`) a gráf `parse_errors`-ába kerül, a hiba előtti és utáni elemek megmaradnak.

## containers
- Mindig van egy `module` container (`<fájlnév>.go`, `doc` = a `package` elé írt komment); a csomag-szintű függvények
  és konstansok ennek tagjai.
- `struct` → `type: class`; a mezők `property`-k (típus, láthatóság), a beágyazott mezők (`Base`, `*Logger`) az
  `extends`-be kerülnek. `interface` → `type: interface` (`abstract`), a metódus-elemek absztrakt metódusok, a
  beágyazott interfészek / típushalmazok (`Fetcher`, `~int | ~string`) az `extends`-ben. `type X int` / `type H
  func()` → `type: type`, `extends` = az alaptípus; `type A = B` → `type: alias`.
- Generikus típus: a típusparaméter-lista (`[T any]`) a `traits`-ben (az outline `uses`-ként írja).
- A receiveres metódus a receiver típusához tartozik (a pointer és a típusparaméter nem része a névnek). Ha a típust
  nem ez a fájl deklarálja (másik fájl ugyanabból a csomagból), egy `type: extension` container fogja össze a fájlbeli
  metódusait. A sortartomány a `type` kulcsszótól indul (egyetlen spec); a csoportos `type ( … )` specjei saját sorral.
- A doc: a deklaráció fölötti, közvetlenül érintkező `//` / `/* */` komment (≤300 kar.); a `//go:…` és `//nolint`
  direktívák kimaradnak.

## methods
- `func` → `kind: function` (modul-container), receiveres `func` → `kind: method`; interfész-elem → `method`,
  `abstract`. `visibility`: nagybetűs név → `public` (exportált), különben `private`.
- `params`: a paraméter-deklarációk forrása (`id int`, `a, b string`, `args ...int`); a receiver kimarad.
  `returnType`: az eredmény forrása, több értéknél zárójelesen (`(*User, error)`, `(n int, err error)`). A közös
  szignatúra-formázó így `Find(id int): (*User, error)` alakot ír (`--signatures`, `--format api`). A típusparaméterek
  a függvény-szignatúrából most nem látszanak (`Map(xs []T, f func(T) U): []U`).
- Név-ütközés (többszörös `init`): `init@L<sor>`. A függvény-literál (`func() { … }`, `go func() { … }()`) nem külön
  elem, hívásai a külső függvényhez tartoznak. A csomag-szintű `const` / `var` inicializálók hívásai a `__module__`
  pszeudo-metódus alatt.

## constants / properties
- Csomag-szinten minden `const` (`kind: const`) és `var` (`kind: static`, így a `--const` is látja) név egy konstans
  (`_` kimarad), rövid értékkel (`iota`, `errors.New("x")`); `const ( a = iota; b )` implicit tagjának nincs értéke.
  `visibility`: nagybetű → `public`. A függvényen belüli deklarációk nem kerülnek be.
- A struct mezői property-k (`type`: a mező típusa, `visibility` a nagybetű alapján); egy sorban több név (`x, y int`)
  több property.

## calls
`kind`: `method_call` (`pkg.Func`, `r.repo.Find`) / `new` (struct-literál `Repo{…}`, `&pkg.T{…}`, fájlbeli típus
konverziója) / `func_call`; `target`: `fmt.Println`, `r.db.Close`, `make`. A `go f()` és a `defer f()` hívása ugyanígy
bekerül (jelölés nélkül).
Kategóriák: `internal_helper` (fájlbeli függvény; `r.x()` a receiveren fájlbeli metódusra), `instantiation` (fájlbeli
típus, struct-literál), `static_call` (`Típus.Metódus()` method expression fájlbeli típuson), `native_go` (beépített
függvény / típus, stdlib-csomag függvénye, gyakori stdlib-metódus: `Error`, `Close`, `Lock`…), `external_call`
(importált nem-stdlib csomag, ismeretlen fogadó, receiveren ismeretlen metódus), `global_helper` (ismeretlen puszta
név: valószínűleg a csomag másik fájljában). A `r.attr.Method()` fogadója `on: "r"` (nincs típus-feloldás). Az
stdlib-felismerés az import-út első szegmensén alapul (`known.rs::STDLIB_ROOTS`).

## strings
Interpretált és raw string-literálok az idézőjelek nélkül (escape-ek feloldás nélkül); az import-utak és a struct-tagek
nem.

## 2. fázis (nincs kész)
Framework-nézetek (Gin / Echo / Chi route-ok, gRPC szolgáltatások, cobra parancsok, `http.HandleFunc`), vezérlési
ágak (`if` / `switch` / `select` / `for`, `defer`/`recover`), kommentek (`TODO`), a `--body` tömör nézet és a
`--const` re-export a forrásszövegből, embedded struct metódus-promóció (a beágyazott típus metódusai az
`extends`-en át), interfész-megvalósítás felismerése (`implements`), goroutine-ok és csatornák (`go` / `defer` /
`chan` jelölése a hívásokon), típus-feloldás (`r.repo` típusa a struct-mezőből), függvényen belüli típusok /
konstansok, a generikus függvények típusparaméterei a szignatúrában, profil-szerepek (`cmd/`, `internal/`, `pkg/`,
`*_test.go`) és `go.mod`-alapú framework-felismerés, `go.mod`-modulútvonalból a belső import fájlútra oldása,
build-tagek (`//go:build`) és a `//go:generate` direktívák.
