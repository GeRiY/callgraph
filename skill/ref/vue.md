# callgraph — Vue

`CG=<skill>/callgraph.sh`, as in [SKILL.md](../SKILL.md) (the core: start there).

## Vue

Same switches. The "methods": `setup` (the top-level code of `<script setup>`), `template` (components,
event handlers, `$t()` / `route()`), functions, `computed`, `watch@L<n>` / `onMounted@L<n>` hooks,
a composable's inner functions (`useX.inner`), Options API `methods` / `computed.x`. Props = properties,
emits = `emit:<name>` (searchable key). The outline lists props once (the `defineProps<X>` interface is not a
separate block; a long default is cut with `…`), `emits (N): a, b` and `models (N): name: Type` on one line each, and
`component_use` paths with a shared prefix as `dir/{atoms/A, organisms/B}.vue` (each `X` = `X/X.vue`). Searchable keys for `--grep` (the JS / TS keys in [js.md](js.md) work too):

```bash
bash $CG --project myapp resources/js --grep 'vue::emit:save'         # who emits 'save'
bash $CG --project myapp resources/js --grep 'Atoms/AtomButton.vue'   # where the component is used
bash $CG --project myapp resources/js --grep 'i18n::bed.locked'       # `t('bed.locked')` in a script, via the project's own t() wrapper too
```
