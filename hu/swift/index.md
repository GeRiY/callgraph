> Swift és SwiftUI térkép: View-típusok komponensként, body és computed property-k, property wrapperek, SwiftUI view-k és modifierek kategóriaként.

Source: https://callgraph.eu/hu/swift/

# Swift és SwiftUI call graph AI agenteknek

A callgraph a [tree-sitter-swift](https://github.com/alex-pinkus/tree-sitter-swift)-tel elemzi a `.swift` fájlokat, és megmutatja az agentnek a típusokat, metódusokat és computed property-ket sortartománnyal, a property wrappereket és minden hívást kategóriánként — a SwiftUI view-k és modifierek saját kategóriát kapnak.

[Telepítés](https://callgraph.eu/hu/#install) [Használat MCP-n át](https://callgraph.eu/hu/mcp/)

## Mit nyer ki a callgraph a Swift-kódból

- Containerek: `class`, `struct`, `enum`, `protocol`, `actor` és `extension`; a beágyazott típusok `Külső.Belső` néven.

- A `View`-nak megfelelő típus `component` — SwiftUI-nézet, mint egy Vue-komponens.

- A fájl-szintű függvények és utasítások a `<Fájl>.swift` `module` containerbe kerülnek.

- Metódusok: `func`, `init`, `deinit`, `subscript` és a computed property-k, így a `var body: some View { … }` a `body` metódus, `some View` visszatérési típussal.

- Property-k a property wrapperrel a típus előtt: `@State String`, `@StateObject TodoViewModel`, `@Published [Todo]`.

- Konstansok (`static let`, fájl-szintű `let`) és enum case-ek.

- Az `if`, `switch` és `guard` ágai (`--format flow`), string-literálok interpolációval, doc-kommentek.

Az `import SwiftUI`-t tartalmazó fájl SwiftUI-fájlként van jelölve.

## SwiftUI-t ismerő hívás-kategóriák

| Kategória | Mikor |
|---|---|
| `swiftui_view` | beépített SwiftUI-nézet: `Text`, `VStack`, `HStack`, `List`, `NavigationStack`, `Button`… |
| `swiftui_modifier` | ismert modifier: `padding`, `font`, `onAppear`, `navigationTitle`, `sheet`, `toolbar`… |
| `component_use` | saját `View`-típus egy nézet-törzsben, pl. `TodoRow(...)` |
| `swiftui_api` | `withAnimation`, `withTransaction` |
| `native_swift` | stdlib, Foundation, Dispatch, Combine: `UUID()`, `JSONDecoder().decode`, `DispatchQueue.main.async`, `append`, `map`… |
| `internal_helper` | ugyanabban a fájlban definiált függvény, vagy `self.f()` |
| `instantiation` | egyéb típus-konstruktor |
| `external_call` | minden más tag-hívás, pl. `viewModel.load()` |

## Parancsok

```
CG=~/.claude/skills/callgraph/callgraph.sh
bash $CG --project myapp <út/a/View>.swift                   # outline: típusok, body, property-k wrapperrel, hívások
bash $CG --project myapp <út/a/View>.swift::body --body      # csak a nézet törzse, sorszámozva
bash $CG --project myapp <út/a/View>.swift --format flow     # if / switch / guard ágak sortartománnyal
```

## Ki hívja ezt a Swift-metódust?

```
bash $CG --project myapp <mappa> --grep '(^|::|\.|->)load$'      # a név minden hívása
bash $CG --project myapp <mappa> --grep '(^|::|\.|->)load$' --depth 2
```

Swiftben a callgraph nem old fel típusokat fájlok között, ezért név szerint keress: a név szerinti minta a `load` minden hívását megtalálja, bármi a fogadó.

## Korlátok

- Nincs típus-feloldás és fájlok közötti szimbólum-tábla: a kategória a fájlbeli nevekből és ismert listákból dől el. Egy másik fájlban deklarált nézet nézet-törzsön kívül `instantiation`.

- A stdlib-metódusok listája név szerinti: egy saját `filter` vagy `append` is `native_swift`.

- Az `#if` minden ága bejárásra kerül; a makrók (`#Preview`, `@Observable`) nincsenek kibontva.

## Kapcsolódó oldalak

[

### Projekt-profil

Felismeri a SwiftUI MVVM projektet és a rétegeit.

](https://callgraph.eu/hu/profile/) [

### locate

Rangsorolt fájlok a feladat kezdéséhez.

](https://callgraph.eu/hu/locate/) [

### MCP-szerver

Ugyanezek a lekérdezések bármely MCP-kliensben.

](https://callgraph.eu/hu/mcp/) [

### Rust

Structok, traitek és `impl`-blokkok.

](https://callgraph.eu/hu/rust/)
