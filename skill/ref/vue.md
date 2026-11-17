# callgraph — Vue

`CG=<skill>/callgraph.sh`, as in [SKILL.md](../SKILL.md) (the core: start there).

## Vue

Same switches. The "methods": `setup` (the top-level code of `<script setup>`), `template` (components,
event handlers, `$t()` / `route()`), functions, `computed`, `watch@L<n>` / `onMounted@L<n>` hooks,
a composable's inner functions (`useX.inner`), Options API `methods` / `computed.x`. Props = properties,
emits = `emit:<name>`. Searchable keys for `--grep` (the JS / TS keys in [js.md](js.md) work too):

```bash
bash $CG --project myapp resources/js --grep 'vue::emit:save'         # who emits 'save'
bash $CG --project myapp resources/js --grep 'Atoms/AtomButton.vue'   # where the component is used
```
