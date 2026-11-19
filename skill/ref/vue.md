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
bash $CG --project myapp resources/js --grep 'Card.vue::emit:save'    # who emits 'save' in Card.vue, and the parents' @save on <Card>
bash $CG --project myapp resources/js --grep 'Atoms/AtomButton.vue'   # where the component is used
bash $CG --project myapp resources/js --grep 'i18n::bed.locked'       # `t('bed.locked')` in a script, via the project's own t() wrapper too
bash $CG --project myapp resources/js --grep 'vue::provide:theme'     # provide('theme') and inject('theme'): one key, both sides
```

**Component contract.** An emit key is scoped to its component (`Card.vue::emit:save`): the emitting function and the
parent's `@save="h"` / `v-on:save` on `<Card>` (resolved through the import) are both edges to it. The outline adds
`slots (N): head, default` (the `<slot>` outlets) and `exposes (N): a, b` (`defineExpose`); `defineProps<ImportedProps>()`
reads the interface from the imported file. A Pinia `defineStore('id', …)` is a `store` block (options style: `actions` /
`getters` are methods, `state` keys properties; setup style: the returned functions), and `useXStore().act()` /
`const s = useXStore(); s.act()` are call edges to it. `--format props` (on a folder or file) checks each component tag
against the child: `⚠ missing required prop 'title'  Parent.vue L12 <Card>`, `⚠ unknown prop 'colr'`, `⚠ unknown slot 'foot'`.
It is lenient: `v-model` / `v-model:x` count as prop + event, kebab-case equals camelCase, no "unknown" when the child
uses `$attrs` / `useAttrs` / `inheritAttrs` / `mixins` or its props type is imported / composed, no "missing" when the
parent spreads `v-bind="obj"`, and standard HTML attributes (`id`, `title`, `data-*`, `aria-*`, …) fall through. With
`unplugin-vue-components` a tag without an import binds to the one `.vue` file of that name (`(auto)`).
