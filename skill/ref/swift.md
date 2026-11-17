# callgraph — Swift / SwiftUI

`CG=<skill>/callgraph.sh`, as in [SKILL.md](../SKILL.md) (the core: start there).

## Swift / SwiftUI

Same switches. Container: `class` / `struct` / `enum` / `protocol` / `actor` / `extension`; a type conforming to `View`
is a `component`; file-level functions are in the `<File>.swift` `module` container. `var body: some View`
and the other computed properties are methods; the property wrapper (`@State`, `@Published`, `@EnvironmentObject`…) is
shown before the property type. Call categories: `swiftui_view`, `swiftui_modifier`, `component_use`,
`native_swift`, `internal_helper`, `external_call`. Details: `docs/LANG-SWIFT.md`.
