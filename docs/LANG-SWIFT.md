# LANG-SWIFT — a Swift / SwiftUI frontend mezőtérképe

Modul: `src/lang/swift/` (`SwiftFrontend`, `LanguageRole::Frontend`, azonosító `swift`, kiterjesztés `.swift`).
Parser: `tree-sitter = "=0.27.1"` + `tree-sitter-swift = "=0.7.4"` (alex-pinkus grammatika, C-forrásból fordul, a
`cc` a buildben). Adapter: `parser/` (`SwiftParser` trait + `TreeSitterSwift`); tree-sitter-típus csak a `parser/`
és a `visitor/` alatt jelenik meg. A hívás-kategorizálás parser-független (`categorize.rs`, `core::Chain`), az ismert
nevek adatfájlja `known.rs`. Mintakészlet: `golden/samples/swift/` (SwiftUI-nézet állapottal, `ObservableObject`
ViewModel, modell-struct extensionnel és enummal, protokoll + actor + fájl-szintű függvény). Golden referencia nincs
(nincs hivatalos Swift-mérce a projektben): a minták elvárt kimenete kézzel ellenőrzött, a tesztek
(`src/lang/swift/**/tests`, `tests/lang_swift.rs`) rögzítik.

## Fájl-szintű keretek
- **file**: a kapott út. **framework**: `"swiftui"`, ha a fájl `import SwiftUI`-t tartalmaz, különben `"swift"`.
  **namespace**, **module**, **sfc**: nincs. **writes**: mindig `[]`.
- **imports**: `import X` → `{"X": "X"}`; `import struct A.B` → `{"B": "A.B"}` (a kulcs az utolsó tag). A Swift-import
  modult nevez meg, nem fájlt: `resolve_module` mindig `None`.
- **Szintaktikai hiba**: a tree-sitter hibatűrő, a fa így is épül; az első `ERROR` / hiányzó token sora
  figyelmeztetésként (`warnings`, en/hu), nem végzetes. A típus-törzsben `ERROR`-ba csomagolt deklarációkat is
  felveszi.

## containers
`{type, name, line, endLine, abstract, final, extends, implements, traits:[], doc}`.
- `type`: `class` / `struct` / `enum` / `protocol` / `actor` / `extension`; a `View`-nak megfelelő (nem protokoll)
  típus `component` (SwiftUI-nézet, mint a Vue-komponens).
- Beágyazott típus neve `Külső.Belső`.
- **extension**: neve a kiterjesztett típus; ha az a fájlban már deklarált (vagy egy korábbi extension elvitte),
  `Típus@L<sor>`, ekkor `extends: [Típus]`. A megfelelt protokollok az `implements`-be kerülnek.
- **Öröklés**: protokollnál minden örökölt típus `extends`; osztálynál az első, ha nem ismert / fájlbeli protokoll
  (`KNOWN_PROTOCOLS`), `extends`, a többi `implements`; enumnál a nyers típus (`Int`, `String`…) `extends`; egyébként
  minden `implements`.
- `abstract` = protokoll; `final` = `final` módosító. `doc` lásd lent.
- **Fájl-szintű container**: ha a fájlban típuson kívüli függvény, változó vagy utasítás van, az első container
  `type: module`, `name: <fájlnév>.swift` (pl. `TodoStore.swift`, a kiterjesztés miatt nem ütközik a típusnevekkel),
  `line 1` – utolsó sor.

## methods
`{container, name, line, endLine, visibility, static, abstract, kind, params[], returnType, doc, body:null}`.
- `kind`: `method` (típusban `func`), `function` (fájl-szintű `func`), `init`, `deinit`, `subscript`, `computed`
  (computed property, pl. `var body: some View { … }` → `body`, `returnType: "some View"`), `top-level`
  (`__module__`: a fájl-szintű utasítások, pl. `main.swift`, az első és az utolsó utasítás sorai között).
- `line`/`endLine`: a deklaráció teljes sortartománya (attribútumokkal, módosítókkal).
- `visibility`: `private` / `fileprivate` / `internal` (alap) / `public` / `open` / `private(set)`.
- `static`: `static` vagy `class` módosító. `abstract`: protokoll-követelmény.
- `params`: a paraméterek forrása ≤60 kar., alapértékkel (`store: TodoStore = UserDefaultsStore()`).
- `returnType`: a `->` utáni típus; computed property-nél a deklarált típus.
- Név-ütközés (overload, több `init`) → `<név>@L<sor>`.

## properties
`{container, name, line, type, visibility, static, readonly, doc}`.
- `var` / `let` tárolt property (property observerrel is), protokoll-property (`var x: T { get }`).
- **type**: az attribútumok (property wrapperek) a típus előtt, szóközzel: `@State String`,
  `@StateObject TodoViewModel`, `@Environment(\.dismiss)`, `@Published [Todo]`. Típus-annotáció híján egyszerű
  következtetés: string / egész / valós / bool literál, illetve `Típus(...)` konstruktor-hívás.
- `readonly` = `let` (protokollnál: nincs `set`). `static` mint a metódusnál.
- Az inicializáló hívásai (`= TodoViewModel()`) a `__top__` hívóhoz kerülnek.

## constants
`{container, kind, name, line, value, visibility, doc}`.
- `const`: `static let` (és `class let`) típusban, illetve fájl-szintű `let`; `value` az inicializáló forrása ≤80 kar.
- `enum_case`: `case a = 1, b` → egy-egy konstans (`value`: nyers érték vagy az asszociált értékek forrása, különben
  `null`). Fájl-szintű `var` property-ként a module containerben.

## calls
`{container, caller, line, endLine?, type, target, on?, category, args?}`; `caller` = a metódus neve, property-
inicializálónál `__top__`, fájl-szintű utasításnál `__module__`. A trailing closure-ök és a beágyazott függvények /
closure-ök hívásai a külső metódushoz tartoznak.
- `type`: `func_call` (`foo()`), `new` (`Típus(...)`), `method_call` (`a.b()`, `.padding()`).
- `target`: puszta hívásnál a név; tag-hívásnál, ha a fogadó csak azonosítókból áll, a teljes út
  (`viewModel.load`, `self.store.save`, `DispatchQueue.main.async`); lánc / implicit tag fogadónál `.név`
  (`.padding`, `.onAppear`, `.decode`). `on` = a fogadó gyökér-azonosítója (nem `self`).
- `line`: a hívott név sora (módosító-láncnál a módosító sora, nem a lánc eleje); `endLine` csak ha több soros.
- `args`: az első ≤4 zárójeles argumentum forrása ≤60 kar. (a trailing closure nem).

### Kategóriák (az első illeszkedő szabály dönt)
| Kategória | Mikor |
|---|---|
| `internal_helper` | a fájlban definiált függvény puszta hívása, vagy `self.f()` / `Self.f()` fájlbeli `f`-re |
| `component_use` | fájlbeli `View`-típus konstruktora (`TodoRow(...)`), vagy ismeretlen típus-konstruktor nézet-törzsben |
| `instantiation` | egyéb típus-konstruktor (fájlbeli nem-nézet típus, ismeretlen típus nem nézet-törzsben) |
| `swiftui_view` | beépített SwiftUI-nézet konstruktora (`Text`, `VStack`, `HStack`, `List`, `NavigationStack`, `Button`…) |
| `native_swift` | stdlib / Foundation / Dispatch / Combine típus konstruktora vagy statikus / lánc-hívása (`UUID()`, `JSONDecoder().decode`, `DispatchQueue.main.async`), szabad függvénye (`print`, `max`, `fatalError`…), gyakori stdlib-metódus bármilyen fogadón (`append`, `filter`, `map`, `uppercased`, `toggle`…) |
| `swiftui_api` | `withAnimation`, `withTransaction` |
| `global_helper` | ismeretlen puszta (kisbetűs) függvény |
| `swiftui_modifier` | ismert módosító-név (`padding`, `font`, `onAppear`, `navigationTitle`, `sheet`, `toolbar`…) lánc / implicit tag / SwiftUI-nézet fogadón, vagy nézet-törzsben |
| `external_call` | minden más tag-hívás (pl. `viewModel.load()`, `store.fetch()`) |

**Nézet-törzs**: a `some View` / `View` / `AnyView` típusú computed property vagy függvény, illetve `@ViewBuilder`
attribútumú tag törzse.

## controlFlow
`{container, caller, type, line, cond, branches[{label, startLine, endLine, fallThrough?}]}`.
- `if`: az `else if` lánc a külső if ágaiként (`if (…)`, `elseif (…)`, `else`); az ág a kulcsszó sorától az `else`
  soráig tart. Több feltétel (`if let a = x, a > 1`) vesszővel.
- `switch`: `cond` = a vizsgált kifejezés; ágonként `case <minták>` / `default`, `fallThrough: true` üres ágra.
- `guard`: egy `else` ág az `else` sorától a guard végéig.
- `cond` ≤100, címke ≤60 kar. Ciklus, `do/catch`, ternary nincs.

## strings / comments / doc
- **strings**: string-literál (egysoros, többsoros, raw), az interpolációk `\(…)`-ként; üres / csak whitespace kimarad;
  ≤300 kar.; `endLine` több sorosnál.
- **comments**: `//` és `/* */` (a `///` és `/** */` docblock kimarad); felvétel: `TODO|FIXME|XXX|HACK` kezdet
  (`tag`) vagy ≥15 kar.; `caller` = a legszűkebb tartalmazó metódus (`line < sor <= endLine`), különben `__top__` a
  legszűkebb containerrel; ≤200 kar.
- **doc**: a deklaráció előtti, hézag nélkül összefüggő `///` sorok (a `///` levágva, soronként `\n`), vagy egy
  közvetlenül előtte álló `/** */` blokk; ≤300 kar. Container, metódus, property, konstans és enum case kapja.

## Ismert korlátok
- Nincs típus-feloldás és fájlok közötti szimbólum-tábla: a kategória a fájlbeli nevekből és az ismert listákból
  (`known.rs`) dől el; egy másik fájlban deklarált nézet nézet-törzsön kívül `instantiation`, egy fájlbeli
  metódussal azonos nevű idegen hívás puszta hívásként `internal_helper`.
- A stdlib-metódus lista név szerinti: egy saját `filter` / `append` metódus tag-hívása is `native_swift`.
- Egy `var a = 1, b = 2` deklarációból csak az első név kerül be; tuple-minta (`let (a, b) = …`) egy property a
  minta szövegével.
- A `#if` fordítási feltételek ágai mind bejárva, feltétel nélkül; makrók (`#Preview`, `@Observable`) nem bomlanak ki
  (`#Preview { … }` fájl-szintű utasításként `__module__` hívásai közé kerül).
- `writes` (property-írások, pl. `self.x = …`), ciklusok és `do/catch` a controlFlow-ban, a `body` mező: nincs.
- A tree-sitter-swift grammatika néhány új szintaxist (pl. a `copy` / `consume` operátort kontextuális névként)
  hibásan értelmezhet; ilyenkor a szerkezet megmarad, a hívás célja eltérhet.
