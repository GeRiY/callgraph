# callgraph — CSS / SCSS / HTML

`CG=<skill>/callgraph.sh`, as in [SKILL.md](../SKILL.md) (the core: start there).

## CSS / SCSS / HTML

`show` gives the structure of the file (rules and selectors, sections, links); `--format classes` joins stylesheet classes
with the templates using them (and the unused ones). Tailwind 4: `@utility` classes and `@theme` token utilities
(`--color-brand-500` -> `bg-brand-500` …) are definitions; `@apply` counts as use; `theme: no such token text-brand-450`
flags a used class whose token is missing (only when the same `brand-…` tokens exist). HTML: `<script src>` /
`<link rel=stylesheet>` show as `import /src/main.ts` lines and callers of the module (external URLs ignored).
Details: `docs/LANG-CSS.md`, `docs/LANG-DOC.md`.
