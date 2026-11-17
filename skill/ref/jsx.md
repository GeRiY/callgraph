# callgraph — React / JSX

`CG=<skill>/callgraph.sh`, as in [SKILL.md](../SKILL.md) (the core: start there). `.jsx` / `.tsx` use the JS / TS
analysis, so the keys and switches in [js.md](js.md) apply too.

## React / JSX

What the outline shows today:

- A function component is a method of the file's `module`: `Card()  L7-18`. A local arrow helper inside it is
  `Card.toggle()`.
- JSX elements that are components (capitalized tags) are `component_use` calls, resolved to the file and export
  (`src/Button.jsx::Button`); lowercase DOM tags are not listed. Use is searchable by that key.
- Hooks are `composable_use` calls: `react::useState`, `react::useEffect`.
- Imports, calls (`module_use  src/api.js::fetchBeds`), `(ref)` edges and TS receiver types work as in JS / TS.
- TSX: a props `interface` / `type` is a separate container with its fields (`number? $count` = optional).
  The component's destructured parameters are not linked to it.

```bash
bash $CG --project myapp src/Card.tsx                              # components used, hooks, calls per function
bash $CG --project myapp src --grep 'src/Button.jsx::Button$'       # where the component is used (JSX line shown)
bash $CG --project myapp src --grep 'react::useEffect$'             # which components run an effect
bash $CG --project myapp src --grep 'src/api.js::fetchBeds$' --depth 2   # who reaches the API call
```

Current limits: the props of a component are not read from its parameters or `props.x` (only a typed props
interface is listed), hook dependency arrays, context and state flow are not analyzed. Deeper React props / hooks
analysis and React code generation are planned; until then read the component file for those details.
