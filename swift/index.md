> Map Swift and SwiftUI code: View types as components, body and computed properties, property wrappers, SwiftUI views and modifiers as call categories.

Source: https://callgraph.eu/swift/

# Swift and SwiftUI call graph for AI agents

callgraph parses `.swift` files with [tree-sitter-swift](https://github.com/alex-pinkus/tree-sitter-swift) and shows your agent types, methods and computed properties with line ranges, property wrappers, and every call by category — with SwiftUI views and modifiers as their own categories.

[Install](https://callgraph.eu/#install) [Use it over MCP](https://callgraph.eu/mcp/)

## What callgraph extracts from Swift

- Containers: `class`, `struct`, `enum`, `protocol`, `actor` and `extension`; nested types as `Outer.Inner`.

- A type conforming to `View` is a `component` — a SwiftUI view, like a Vue component.

- File-level functions and statements go into a `<File>.swift` `module` container.

- Methods: `func`, `init`, `deinit`, `subscript` and computed properties, so `var body: some View { … }` is the method `body` with return type `some View`.

- Properties with their property wrapper before the type: `@State String`, `@StateObject TodoViewModel`, `@Published [Todo]`.

- Constants (`static let`, file-level `let`) and enum cases.

- Branches of `if`, `switch` and `guard` (`--format flow`), string literals with interpolations, doc comments.

A file that contains `import SwiftUI` is marked as SwiftUI.

## SwiftUI-aware call categories

| Category | When |
|---|---|
| `swiftui_view` | a built-in SwiftUI view: `Text`, `VStack`, `HStack`, `List`, `NavigationStack`, `Button`… |
| `swiftui_modifier` | a known modifier: `padding`, `font`, `onAppear`, `navigationTitle`, `sheet`, `toolbar`… |
| `component_use` | your own `View` type used in a view body, e.g. `TodoRow(...)` |
| `swiftui_api` | `withAnimation`, `withTransaction` |
| `native_swift` | stdlib, Foundation, Dispatch, Combine: `UUID()`, `JSONDecoder().decode`, `DispatchQueue.main.async`, `append`, `map`… |
| `internal_helper` | a function defined in the same file, or `self.f()` |
| `instantiation` | other type constructors |
| `external_call` | other member calls, e.g. `viewModel.load()` |

## Commands

```
CG=~/.claude/skills/callgraph/callgraph.sh
bash $CG --project myapp <path/to/View>.swift                   # outline: types, body, properties with wrappers, calls
bash $CG --project myapp <path/to/View>.swift::body --body      # just the view body, line-numbered
bash $CG --project myapp <path/to/View>.swift --format flow     # if / switch / guard branches with line ranges
```

## Who calls this Swift method?

```
bash $CG --project myapp <folder> --grep '(^|::|\.|->)load$'      # every call of the name
bash $CG --project myapp <folder> --grep '(^|::|\.|->)load$' --depth 2
```

Swift has no cross-file type resolution in callgraph, so search by name: the name pattern finds every call of `load`, whatever the receiver.

## Limits

- No type resolution and no cross-file symbol table: categories come from the names in the file and known lists. A view declared in another file counts as `instantiation` outside a view body.

- The stdlib method list is by name: your own `filter` or `append` is also `native_swift`.

- `#if` branches are all walked; macros such as `#Preview` and `@Observable` are not expanded.

## Related

[

### Project profile

Recognises a SwiftUI MVVM project and its layers.

](https://callgraph.eu/profile/) [

### locate

Ranked files to start a task with.

](https://callgraph.eu/locate/) [

### MCP server

The same queries in any MCP client.

](https://callgraph.eu/mcp/) [

### Rust

Structs, traits and `impl` blocks.

](https://callgraph.eu/rust/)
