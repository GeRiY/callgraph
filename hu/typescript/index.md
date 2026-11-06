> JS, TS és Vue SFC outline: importok alias-feloldással, props, emits, setup, template-komponensek, típusos fogadón át hívott metódusok és hívóik.

Source: https://callgraph.eu/hu/typescript/

# TypeScript, JavaScript és Vue call graph

A callgraph az [oxc](https://github.com/oxc-project/oxc)-vel elemzi a JavaScript-, TypeScript- és Vue single-file komponenseket, és megadja az agentnek a modulokat, függvényeket, osztályokat, a komponensek propsait és emitjeit, a template-használatot és minden hívást kategóriánként — és hogy ki használ egy függvényt, komponenst, route-ot vagy fordítási kulcsot a projektben.

[Telepítés](https://callgraph.eu/hu/#install) [Használat MCP-n át](https://callgraph.eu/hu/mcp/)

## Mit nyer ki a callgraph

Fájlok: `.js` `.mjs` `.cjs` `.jsx` `.ts` `.tsx` `.vue`.

- Modulok, függvények és osztályok; TS `interface`-ek és az osztályok `implements`-e.

- Importok alias-feloldással a `jsconfig.json` / `tsconfig.json` alapján (a `@/` alias is).

- Az objektum-literálok függvényei külön metódusként: `const api = { fetch() {} }` → `api.fetch`.

- `if` / `switch` ágak, string- és template-literálok.

## Vue single-file komponensek

A `.vue` fájl egy `component`. A „metódusai” azok, amik alapján egy agent eligazodik benne:

- `setup` — a `<script setup>` top-level kódja.

- `template` — a használt komponensek, eseménykezelők, `$t()`- és `route()`-hívások.

- Függvények, `computed`, `watch@L<n>` / `onMounted@L<n>` hookok, egy composable belső függvényei (`useX.belso`), és az Options API `methods` / `computed.x` elemei.

- A props property-ként, az emitek `emit:<név>` alakban látszanak.

A hívások kategóriái között van a `vue_api`, `composable_use`, `component_use`, `vue_emit`, `inertia`, `i18n_helper`, `route_helper` (Ziggy), `http_call` (axios), `module_use` és a `package_use`.

## Ki használja ezt a függvényt, komponenst vagy kulcsot?

Ugyanaz a `--grep` kapcsoló frontend-specifikus kulcsokat is ért:

```
CG=~/.claude/skills/callgraph/callgraph.sh
bash $CG --project myapp resources/js --grep 'useForm.js::submit'     # <fájl>::<export>, import-feloldással, @/ aliasszal
bash $CG --project myapp resources/js --grep 'route::orders.show'     # route-használat
bash $CG --project myapp resources/js --grep 'i18n::orders.title'     # fordítási kulcs helye
bash $CG --project myapp resources/js --grep 'vue::emit:save'         # ki emitál 'save'-et
bash $CG --project myapp resources/js --grep 'Atoms/AtomButton.vue'   # hol használják a komponenst
bash $CG --project myapp resources/js --grep 'api.fetch'              # objektum-literál metódusa
```

A `--strings` a Vue template-kifejezésekben (`{{ … }}`, `:title="…"`, `@click="…"`) lévő szövegeket is megtalálja.

## TypeScript: hívások típusos objektumokon át

Ha a típus le van írva, a callgraph kiszámolja, melyik osztályhoz megy egy metódushívás: típusos paraméter (`repo: Repo`), típusos osztály-property (`private repo: Repo`, a `constructor(private repo: Repo)` is), `this.repo = new Repo()` vagy `const x = new Repo()`. Így a `this.repo.find()` `Repo::find` lesz, és a `--grep 'Repo::find'` megtalálja. Az interfészen át tett hívások `via Resolver` jelölést kapnak.

```
bash $CG --project myapp resources/js --grep 'PlaylistResolver::getPlayables'
```

Ha a típus nem egyértelmű — nincs típus, `any`, unió, mint a `Repo | null`, generikus `T` —, a hívás feloldatlan marad; ezeket név szerint keresd: `--grep '(^|::|\.|->)find$'`.

## Korlátok

- A dinamikus hívások (`obj[key]()`) és a futásidőben regisztrált komponensek statikusan nem oldhatók fel.

- A hivatkozott típusnévvel megadott `defineProps<Props>()` nem listáz propsot (a típus nincs feloldva).

## Kapcsolódó oldalak

[

### PHP, Laravel és Yii

Egy Laravel + Vue / Inertia alkalmazás backend-oldala.

](https://callgraph.eu/hu/php/) [

### Projekt-profil

Felismeri a Vue / React frontendet és a rétegeit.

](https://callgraph.eu/hu/profile/) [

### locate

Egy fájlnév, pl. `checkout.vue` is mehet a kifejezések közé.

](https://callgraph.eu/hu/locate/) [

### MCP-szerver

Ugyanezek a kulcsok a `find_callers` eszközben is működnek.

](https://callgraph.eu/hu/mcp/)
