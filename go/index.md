> Map Go code: structs, interfaces, receiver methods, generics, signatures with several return values, categorized calls, Gin / Echo / Chi / gRPC / GORM detection.

Source: https://callgraph.eu/go/

# Go call graph for AI agents

callgraph parses `.go` files with [tree-sitter-go](https://github.com/tree-sitter/tree-sitter-go) and shows your agent structs, interfaces, functions and methods with line ranges, signatures, and every call by category.

[Install](https://callgraph.eu/#install) [Use it over MCP](https://callgraph.eu/mcp/)

## What callgraph extracts from Go

- Every file gets a `<file>.go` `module` container with its package-level functions and constants; the comment above `package` is its doc, and the package name is shown as the namespace.

- `struct` as a class: fields as properties, embedded types (`Base`, `*Logger`) as `extends`. `interface` with abstract methods and embedded interfaces or type sets.

- Defined types (`type Level int`, `type Handler func()`) and aliases (`type A = B`). Generic types show their type parameters (`[T any]`).

- Methods with receivers belong to the receiver type, pointer or not. If the type is declared in another file of the package, the methods of this file are grouped under an `extension` container.

- Signatures with several return values: `Find(id int): (*User, error)` (`--signatures`, `--format api`). A capitalised name is public (exported), a lower-case one private.

- Package-level `const` and `var`, imports (alias, blank `_` and dot `.` included), the doc comment right above a declaration, string literals, and syntax errors with a warning.

- Several `init` functions show as `init@L<line>`; a function literal is not a separate item, its calls belong to the outer function.

A file that imports Gin, Echo, Fiber, Chi, gorilla/mux, gRPC, GORM or cobra is marked with that framework. `vendor` and `testdata` are skipped by default; `_test.go` files count as tests in [locate](https://callgraph.eu/locate/).

## Call categories

| Category | When |
|---|---|
| `internal_helper` | a function defined in the same file, or `r.f()` on the receiver for a method of the same file |
| `instantiation` | a struct literal such as `Repo{…}` or `&pkg.T{…}`, or a conversion to a type of the same file |
| `static_call` | `Type.Method()` (method expression) on a type of the same file |
| `native_go` | builtins, standard-library packages and common methods: `make`, `fmt.Println`, `errors.New`, `Close`, `Lock`… |
| `external_call` | an imported non-stdlib package, an unknown receiver, an unknown method on the receiver |
| `global_helper` | an unknown bare name, probably defined in another file of the same package |

The calls behind `go` and `defer` are included like any other call.

## Commands

```
CG=~/.claude/skills/callgraph/callgraph.sh
bash $CG --project myapp <path/to>/cart.go                  # outline: structs, interfaces, functions, calls
bash $CG --project myapp <path/to>/cart.go --signatures     # with the signature on each method line
bash $CG --project myapp <path/to>/cart.go::Add --body      # just that method, line-numbered
bash $CG --project myapp <path/to>/cart.go --format api     # the exported surface only
```

Output of the outline for a small file

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

## Who calls this Go function?

```
bash $CG --project myapp <folder> --grep '(^|::|\.|->)helper$'      # every call of the name
bash $CG --project myapp <folder> --grep '(^|::|\.|->)helper$' --depth 2
```

Go has no type resolution in callgraph, so search by name: the pattern finds every call of `helper`, whatever the receiver. Each match shows the call's own source line.

## Limits

This is the first phase of Go support.

- No type resolution: the receiver of `r.repo.Find()` stays `r`, an internal import is not resolved to a file, and the type parameters of a generic function are not shown in its signature.

- Methods promoted from an embedded struct and interface implementations are not followed.

- No framework views (Gin / Echo / Chi routes, gRPC services, cobra commands) and no `--format flow` branches yet; `//go:build` tags are not evaluated.

- Declarations inside a function are not listed.

## Related

[

### locate

Ranked files to start a task with.

](https://callgraph.eu/locate/) [

### Project profile

How an unfamiliar project is built, in about 15 lines.

](https://callgraph.eu/profile/) [

### MCP server

The same queries in any MCP client.

](https://callgraph.eu/mcp/) [

### Python

Classes, decorators and signatures.

](https://callgraph.eu/python/)
