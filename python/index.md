> Map Python code: classes with bases and decorators, functions and methods, signatures, imports, categorized calls, Django / Flask / FastAPI detection.

Source: https://callgraph.eu/python/

# Python call graph for AI agents

callgraph parses `.py` and `.pyi` files with [tree-sitter-python](https://github.com/tree-sitter/tree-sitter-python) and shows your agent classes, functions and methods with line ranges, signatures, and every call by category.

[Install](https://callgraph.eu/#install) [Use it over MCP](https://callgraph.eu/mcp/)

## What callgraph extracts from Python

- Every file gets a `<file>.py` `module` container with its module-level functions and constants; the module docstring is its doc.

- Classes with their bases (`extends`) and decorators (`uses @dataclass`); nested classes as `Outer.Inner`; the line range starts at the first decorator. An `ABC` base marks the class abstract.

- Functions and methods: `def` and `async def`, `@staticmethod`, `@classmethod`, `@property` (also `@cached_property` and setters), `@abstractmethod`. A leading underscore means private. The first string literal of the body is the docstring.

- Signatures with defaults and annotations: `add(n: int): int` (`--signatures`, `--format api`); `self` and `cls` are left out.

- Constants: `UPPER_CASE` names, every annotated assignment and `__all__`; class attributes as properties, with the type from the annotation or the value.

- Imports in all forms, relative ones too (`from .m import x`), also inside functions and `try` / `if` blocks.

- String literals (the static parts of f-strings) and syntax errors: a file that does not parse still shows everything before and after the broken line, plus a warning.

A file that imports `django`, `flask` or `fastapi` is marked with that framework. `__pycache__`, `venv`, `.venv` and `site-packages` are skipped by default.

## Call categories

| Category | When |
|---|---|
| `internal_helper` | a function defined in the same file, or `self.f()` / `cls.f()` on a method of the same file |
| `instantiation` | a capitalised bare name, `Foo()` |
| `static_call` | `Class.method()` on a class defined in the same file |
| `native_python` | builtins, standard-library modules and common builtin methods: `len`, `os.path.join`, `append`… |
| `external_call` | an imported non-stdlib package, `super().x()`, or a receiver callgraph cannot name |
| `global_helper` | an unknown bare name, probably imported with `*` or defined elsewhere |

## Commands

```
CG=~/.claude/skills/callgraph/callgraph.sh
bash $CG --project myapp <path/to>/cart.py                  # outline: classes, functions, properties, calls
bash $CG --project myapp <path/to>/cart.py --signatures     # with the signature on each method line
bash $CG --project myapp <path/to>/cart.py::add --body      # just that method, line-numbered
bash $CG --project myapp <path/to>/cart.py --format api     # the public surface only
```

Output of the outline for a small file

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

## Who calls this Python function?

```
bash $CG --project myapp <folder> --grep '(^|::|\.|->)helper$'      # every call of the name
bash $CG --project myapp <folder> --grep '(^|::|\.|->)helper$' --depth 2
```

Python has no type resolution in callgraph, so search by name: the pattern finds every call of `helper`, whatever the receiver. Each match shows the call's own source line.

## Limits

This is the first phase of Python support.

- No type resolution: the receiver of `self.repo.save()` stays `self`, and an imported module is not resolved to a file.

- Framework detection looks at the imports of the file only; there are no Django URL / model views or Flask / FastAPI route views yet, and no `--format flow` branches.

- A nested function or lambda is not a separate item: its calls belong to the outer function.

- The builtin-method list is by name: your own `append` is also `native_python`.

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

### Go

Structs, interfaces and receiver methods.

](https://callgraph.eu/go/)
