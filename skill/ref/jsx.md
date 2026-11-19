# callgraph — React / JSX

`CG=<skill>/callgraph.sh`, as in [SKILL.md](../SKILL.md) (the core: start there). `.jsx` / `.tsx` use the JS / TS
analysis, so the keys and switches in [js.md](js.md) apply too.

## Outline

- A function component is a method of the file's `module`: `Card()  L7-18`, with its props under it:
  `props (4): title*, size, onSave*, children` (`*` = required, `…` at the end = more props may be passed, `+N` =
  capped list). A local arrow helper inside it is `Card.toggle()`.
- JSX elements that are components (capitalized tags) are `component_use` calls, resolved to the file and export
  (`src/Button.jsx::Button`); lowercase DOM tags are not listed.
- Hooks are `composable_use` calls: `react::useState`, `react::useEffect`. A project hook (`function useCart()` /
  `const useCart = …`) that runs a React hook is marked `[hook]` in its own outline; its callers are
  `composable_use  src/hooks/useCart.ts::useCart`.
- Context: `createContext()`, `<XContext.Provider>`, `<XContext.Consumer>` and `useContext(XContext)` share one key
  `react::context:<defining file>::XContext`, so `--grep` finds the provider and every consumer.
- Imports, calls (`module_use  src/api.js::fetchBeds`), `(ref)` edges and TS receiver types work as in JS / TS.

Props come from the first parameter: destructuring (`size = 'md'` = optional, `...rest` = open), a TS type (inline,
local or imported `interface` / `type`, `extends`, `A & B`, `PropsWithChildren<T>`), `FC<Props>` /
`React.FC<Props>`, `memo(…)`, `forwardRef<El, Props>(…)`, `Comp.propTypes` (`.isRequired`), `Comp.defaultProps`, and
the `props.x` reads of an untyped `props` object.

## Props view

`--format props` checks the JSX uses of the queried files against the props the component declares:

```
props: 14 component uses checked, 3 missing required, 2 unknown props
⚠ missing required prop 'onSave'  src/pages/Home.tsx L19 <Card>
⚠ unknown prop 'colour'  src/pages/Home.tsx L19 <Card>
```

Conservative: `{...spread}` skips the "missing" check, an open prop set (`...rest`, `HTMLAttributes` /
`ComponentProps<'button'>` / `Omit<…>` / unresolved or union types, an untyped `props` object, a component without
parameters) skips the "unknown" check, and `key`, `ref`, `children`, `className`, `style`, `data-*`, `aria-*` are
never unknown (JSX children count as `children`). Only a component bound statically is checked: a function component
of the same file, or an import of a project file whose export is the component itself (a HOC-wrapped
`export default connect(…)(X)` is not).

## Routes (react-router)

`--format routes` reads `<Routes><Route path element /></Routes>` trees (nested, `index`), `createBrowserRouter([…])`
/ `createHashRouter` / `useRoutes([…])` arrays (`element`, `Component`, `lazy`, `children`), and flags literal
`<Link to="/x">` / `<NavLink>` / `<Navigate>` / `navigate('/x')` paths that no route matches
(`⚠ unknown route path '/x'  file L12`; `:param` and `*` match). No warning without a table, with a non-literal
route path, or with `addRoute`. Client `fetch` / `axios` URLs are matched to server routes as for Vue.

## Inertia + React

`@inertiajs/react` projects work like the Vue ones: `Inertia::render('Events/Show', …)` resolves to
`resources/js/Pages/Events/Show.tsx` (or `.jsx`), the page's props are those of its default-exported component (the
`--format inertia` view flags required props the controller never passes), `route('x')` in `<Link href>` /
`router.visit` is a Ziggy edge, and `trace` reaches the page and its child components.

```bash
bash $CG --project myapp src/Card.tsx                              # components used, hooks, props, calls per function
bash $CG --project myapp src --format props                        # missing / unknown props at JSX uses
bash $CG --project myapp src --grep 'src/Button.jsx::Button$'       # where the component is used (JSX line shown)
bash $CG --project myapp src --grep 'react::context:src/ctx/Cart.tsx::CartContext$'   # provider + consumers
bash $CG --project myapp src --format routes                       # react-router table, unknown link paths
bash $CG --project myapp src --grep 'src/api.js::fetchBeds$' --depth 2   # who reaches the API call
```

Limits: class components, `useState` setter / state flow and hook dependency arrays are not analyzed; a props type
from a package, a union or a mapped type makes the set open (no "unknown" findings); a context re-exported through
a barrel file keeps the key of the barrel's import; `<Link to>` with a template or variable path is not checked.

## Generate

`gen` writes React too (see [gen.md](gen.md)): `react:Card>props(title:str,size?:'sm'|'md'='md')+ref:count:int=0+fn:inc():void{ setCount(count + 1) }+jsx>div.card>h1{{ title }}+button[@click=inc]{+1}`
with `--write src/components/Card.tsx` (`.jsx`: plain JS + JSDoc). It makes a function component with a typed `interface CardProps`,
destructured props with defaults, `useState`, handlers and JSX (`class` -> `className`, `:x=v` -> `x={v}`, `@click=f` -> `onClick={f}`,
`v-for="x in xs"` -> `xs.map`, `v-if=c` -> `c && (…)`; `emits(save:int)` -> an `onSave` callback prop; `on:mounted{…}` -> `useEffect`).
Semicolons, indent, TS / JS and `export default` vs named export follow the project's components; components found in the
project are imported (`Link` / `Head` from `@inertiajs/react`). `--into x.tsx` adds props / state / handlers / elements to a component;
`hook:useCart>ref:items:list<Item>=[]+fn:add(…){…}` with `--write src/hooks/useCart.ts` writes a custom hook; in an Inertia + React Laravel
project `laravel:Fee>create(…)+index` writes `Pages/<Models>/*.tsx` (`useForm`, `route()`, your own input / label / error / button components).
