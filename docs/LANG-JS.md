# LANG-JS — a js_graph.mjs mezőtérkép (a Rust-port alapja)

Forrás: `~/.claude/skills/callgraph/js/js_graph.mjs` (576 sor). Kimenet: fájlonként egy FileGraph-JSON
(`file` = abszolút út). Golden: `golden/ref-js.jsonl` (679 fájl), `golden/ref-js-samples.jsonl` (8 minta);
sor-alak `{"path","graph"}` (mint a PHP-nél). Parser: `@babel/parser` (jsx, typescript csak `.ts/.tsx` ill. `lang=ts|tsx`
blokkon, decorators-legacy, importAttributes; `errorRecovery`, `attachComment`), `.vue`: `@vue/compiler-sfc`.

## Függvénytérkép
| Függvény | Sorok | Feladat |
|---|---|---|
| konstansok | 20–33 | EXTS `.js .mjs .cjs .ts .vue .jsx .tsx`; SKIP_DIRS `node_modules vendor public build dist .git storage cache`; NATIVE_GLOBALS, VUE_MACROS, HOOK_RE, FN_WRAPPERS |
| findConfig | 35–55 | alias-config keresés |
| tryFile | 56–61 | kiterjesztés-próba |
| resolveSource | 62–78 | import-forrás feloldás |
| lineOf/endOf/srcOf/isFn/keyName/memberRoot/docOf | 81–101 | segédek |
| categorize | 104–113 | import → hívás-kategória |
| propsFrom / emitsFrom | 116–146 | props, emits |
| analyzeTemplate | 151–211 | Vue template |
| analyzeFile | 214–258 | fájl, SFC-blokkok, container |
| analyzeProgram | 260–538 | addMethod, nested, optionsApi, topStmt, scanFns, resolveCallee, walk, kommentek |
| analyzeCached / collect / main | 541–576 | cache (mtime+script mtime), fájlgyűjtés, kimenet |

## Fájl-szintű keretek
- **Gyűjtés** (`collect` 555–565, main 566–576): mappa rekurzió, nevek rendezve (JS `sort`), `SKIP_DIRS` és a `.`-tal kezdődő nevek kimaradnak, `*.min.js` kimarad. Hibás fájl: stderr, a sor kimarad.
- **file**: a kapott abszolút út. **module**: `path.relative(findConfig(dir).root, file)` (analyzeFile 217).
- **framework** (217): `.vue` → `"vue"`, minden más `"js"`. **namespace**: mindig `null`. **writes**: mindig `[]`.
- **Blokkok** (analyzeFile 221–242): `.vue`: `descriptor.script` majd `descriptor.scriptSetup` külön Babel-parse, `startLine` = blokk-kezdősor (a sorszámok így fájl-abszolútak); nem-vue: egy blokk, `ts` = `.ts/.tsx`. A `lang` `/^tsx?$/`. Parse-hiba: `parseError` mező, a blokk kihagyva. Hibás SFC script nélkül: `parseError`.
- **sfc** (`.vue`, 230): `{template:{line,endLine}|null, scriptSetup:bool, styles:<db>}`.
- **containers[0]** (245): `type` = `component` (.vue) / `module`, `name` = fájlnév kiterjesztés nélkül, `line 1`, `endLine` = sorok száma (`split('\n')`), `extends/implements/traits` üres, `doc` lásd lent. Osztályonként további container (`type:'class'`, `extends:[szuperosztály-forrás]`, `implements:[TS implements-nevek forrása]`, topStmt ClassDeclaration); TS `interface X extends Y, Z` → `type:'interface'` container `extends:[Y, Z]`-vel (a metódus-szignatúrái nem kerülnek `methods`-ba). Az `extends`/`implements` a `TypeHierarchy`-t tölti (`via <Interfész>` hívó-keresés).
- **container.doc** (262–267): az első top-level utasítás vezető `/** */` docblockja, különben az első komment, ha blokk-komment és a blokk-kezdősor+1 előtt van (`*`-ok levágva, ≤300 kar.).

## imports (analyzeProgram 355–377)
Top-level `ImportDeclaration`. Kulcs = helyi név; érték = `display` + `''` (default) / `::*` (namespace) / `::<imported>`.
`display` = `resolveSource`: relatív (`.`-os) → abszolútra, `paths` alias (tsconfig/jsconfig; a `*` végű mintát előtagként illeszti, első target) → abszolútra; ha lokális: `tryFile` (p, +.js, +.ts, +.vue, +.mjs, p/index.js, p/index.ts), majd `path.relative(cfg.root, f)` (a `@/` alias így `resources/js/...`); csomag: a forrás változatlanul. `findConfig`: a fájl mappájától felfelé az első `jsconfig.json`, majd `tsconfig.json` (komment/trailing-comma eltávolítás, `compilerOptions.baseUrl`/`paths`); ha nincs, az első `package.json`-os mappa (vagy fs-gyökér) a root, `paths` üres. Cache mappánként. Az import emellett `bindings`-et tölt (`resolved` = `display::<imported|helyi név>`, `category` = categorize, `component` = `.vue`-ra végződik).

## Hívás-kategóriák
`categorize` (104): forrás `vue` → `vue_api`; `@inertiajs/*` → `inertia`; `vue-i18n`/`laravel-vue-i18n` → `i18n_helper`; `ziggy-js` vagy `ziggy` a névben → `route_helper`; `axios` vagy `…AxiosHandler.js` → `http_call`; lokális és (`/Composable(s)/` az útban vagy `use[A-Z]…` név) → `composable_use`; `use[A-Z]…` bármi → `composable_use`; lokális → `module_use`; különben `package_use`.
`resolveCallee` (399–435) további: `internal_helper` (metódusnév / fájlon belüli függvény / `this.metódus`), `vue_macro` (VUE_MACROS, `resolved:'vue::<név>'`), `route_helper` (`route`, `resolved:'ziggy::route'`), `i18n_helper` (`t`,`$t`), `native_js` (NATIVE_GLOBALS), `global_helper` (ismeretlen azonosító), `external_call` (ismeretlen tag-hívás, `on:'<expr>'` nem azonosító gyökérnél), `vue_emit`, `vue_props`, `instantiation` (NewExpression, nem natív), `component_use` (template), `inertia` (`this.$inertia/$page`). A golden-ben előfordul még 6 hívás `category` nélkül (template `exprCalls` ágak) — a Rust-portnak ezt is reprodukálnia kell.

## methods (addMethod 273, nested 285, optionsApi 296, topStmt 316)
Alak: `{container,name,line,endLine,visibility,static,abstract:false,kind,params[],returnType:null,doc,body:null}`; `params` = a paraméter-forrás ≤60 kar., `doc` = docblock (stmt, majd fn), a `line/endLine` a **statement** (nem a függvény-node) sorai. Név-ütközés → `<név>@L<sor>`.
- **Top-level** (`topStmt`): `FunctionDeclaration` → `név`; `const x = fn|arrow` → `x` (egy deklarátoros stmt-nél a stmt, többnél a declarator sorai); `const x = computed|useMemo|debounce|throttle|useDebounceFn|useThrottleFn(fn)` → `x`, `kind` = a wrapper neve; export → `visibility:'public'`, különben `private`; `kind:'function'`.
- **Hook**: `onXxx|watch|watchEffect|watchPostEffect|watchSyncEffect(…, fn)` kifejezés-stmt → `<hook>@L<sor>`, `kind:'hook'` (az utolsó függvény-argumentum).
- **Beágyazott** (`nested`): a függvény törzsében közvetlen `function`/`const x = fn` → `külső.belső`, `kind:'inner'` (csak egy szint).
- **Options API** (`optionsApi`, `export default {}` vagy `export default call({…})`): `props`/`emits` külön; függvényes kulcs → név = kulcs, `kind:'option'` (`setup` → `kind:'setup'` + nested); `methods.x` → `x` (`kind:'methods'`); `computed.x`, `watch.x` (`kind` = `computed`/`watch`), objektum-értéknél az első függvény (get/handler).
- **Osztály**: `ClassMethod`/függvény-értékű `ClassProperty` → `container` = osztálynév, `kind:'method'`, `static`.
- **Objektum-literál** (Rust-port bővítés, a régi motorban nincs): top-level `const api = { fetch() {…}, save: () => …,
  load: function () {…} }` (export-tal is) → `api.fetch`, `api.save`, `api.load` (`kind:'method'`, a tulajdonság
  sortartományával; láthatóság mint a deklarációé), a beágyazott objektum egy szintig (`api.users.list`). A
  Vue-kulcs nélküli `export default { … }` (nincs `name`/`props`/`data`/`methods`/`computed`/`watch`/`setup`/hook… kulcs)
  ugyanígy, előtag nélkül (`fetch`, `helpers.fmt`); Vue-kulccsal az Options API szabálya él. A fájlon belüli
  `api.fetch()` hívás `internal_helper` (`target:'api.fetch'`), a metódus törzsében lévő hívások `caller`-e `api.fetch`.
- `export default function` → `default`/névvel; `export default class` → topStmt.
- **Pszeudo-metódusok**: `setup` (`<script setup>` blokk) vagy `__module__` (egyéb blokk) — csak ha van benne top-level `caller`-es hívás/elágazás; `kind:'top-level'`, a blokk sortartományával. `template` (`.vue` template, `kind:'template'`, a template sortartománya) — mindig, ha van template. A lista `line` szerint rendezett (stabil).

## properties / constants
- **properties** `{container,name,line,type,readonly,static:false,kind:'prop'}` — `propsFrom` (116): `defineProps`/`props:` Options; objektum-alak (`type` = a `type` kulcs forrása ≤40, `required` → `readonly:true`; rövid alak: érték-forrás), tömb-alak (`type:'mixed'`), TS típus-literál / interface-törzs (`readonly` = `!optional`). **`defineProps<Props>()` hivatkozott típusnévnél üres** (nincs feloldás — lásd `golden/samples/js/SetupTs.vue`).
- **constants** `{container,name,line,value,kind}`: `const` (top-level `UPPER_SNAKE` név, literál/tömb/objektum init, érték ≤80 kar.); `enum_case` (`Enum.Tag`, init-forrás vagy `null`); `emit` (`emit:<név>`, `value:null`, `emitsFrom` 139: tömb, objektum, TS hívás-szignatúra regex `(e: 'x'` és `'x': [` regex).

## calls (`walk` 436–526 + analyzeTemplate)
Alak: `{container,caller,line,type,target,on?,category,resolved?,args?,handler?}`; `type` = `func_call` / `method_call` / `new` / `component`. `caller` = a legközelebbi metódus-ős (`methodOf`), különben `setup`/`__module__`. `args` = az első ≤4 argumentum forrása ≤60 kar. `VariableDeclarator`-ban `const x = hívás()` / `await hívás()` / destrukturálás a `bindings`-be kerül (`resolved:'<callee>()'`, `.kulcs`; `t` → `i18n_helper`; `defineEmits` → `vue_emit`, `defineProps|withDefaults` → `vue_props`). Literál első argumentum: `vue_emit` → `resolved:'vue::emit:<n>'`, `i18n_helper` → `i18n::<kulcs>`, `route_helper` → `route::<név>`. Tag-hívás: gyökér azonosító → `target` = `gyökér.rész.rész` (ha binding/natív) vagy az utolsó rész (`external_call`, `on`=gyökér); `this.x` → `internal_helper`/`inertia`/`i18n`/`vue_emit`/`external_call` (`target:'this.…'`).
**Fogadó-típus (TS, `calls/receivers.rs`)**: a `x.m()` / `this.x.m()` / `x!.m()` hívás `resolved` = `Típus::m`, ha a fogadó típusa egyértelmű: típusos paraméter (`(repo: Repo)`, a konstruktor paraméter-property-je `constructor(private repo: Repo)` is), típusos osztály-property (`private repo: Repo`, `repo!: Repo`), `this.repo = new Repo()` / `repo = new Repo()` property-érték, `const x: Repo = …`, `const x = new Repo(…)`. A típusnév a fájl top-level osztálya / interfésze (`category:'internal_helper'`) vagy importált név (az import exportált neve, `import { Repo as R }` → `Repo`; `import * as m` → `m.Repo` → `Repo`; a kategória az importé, pl. `module_use` / `package_use`). Egy név a függvényben (osztályban) csak akkor típusos, ha minden kötése ugyanazt adja (az annotáció dönt; különben a `new` értékadások egyezése); a típus nélkül deklarált belső név (pl. `(r) => r.m()`) árnyékolja a külsőt. Feloldatlan marad: generikus paraméter, `any`/`unknown`, unió (`Repo | null`), nem importált / nem a fájlban deklarált típus (`Map`, `HTMLElement`), mélyebb lánc (`this.a.b.m()`), a nem-metódus `function`-ön belüli `this`, Options API `this`. JS-ben nincs `?::m` (a Rusttal ellentétben): a tömb-/DOM-metódusok miatt a `Típus::m` keresés túl zajos lenne. A `this.m()` saját osztálymetódusra `resolved:'Osztály::m'` (`internal_helper`).
`scanFns` (378–397) előre gyűjti a fájlon belül definiált függvényneveket (`localFns`) és az `{emit}` destrukturált paramétert (`vue_emit` binding).
**Template** (`analyzeTemplate`, `caller:'template'`): elem `Pascal`/importált tag → `component_use` (`type:'component'`, `resolved` az import); direktívák: `exprCalls` regex a kifejezésen (`$t`,`t`,`$tc` → i18n; `route` → route; `$emit`,`emit` → vue_emit; metódus → `internal_helper`; import → `module_use`; más → `external_call`); `@event` → `strings` `@<esemény>`; puszta függvénynév-kezelő → `internal_helper` + `handler:true`.

## controlFlow
`{container,caller,type,line,cond,branches[{label,startLine,endLine,fallThrough?}]}`. `if` (az `else if` lánc a külső if-nél, ágak: `if (…)`, `elseif (…)`, `else`), `switch` (`case …`/`default`, `fallThrough` üres case-re). Template: `v-if` / `v-else-if` (`cond` ≤100, ág-címke `v-if="…"` ≤60, sor = az elem sortartománya). Más vezérlés (for, ternary, try) nincs.

## strings
`{container,caller,line,value}`: `StringLiteral` (nem import/export-forrás, nem objektum-kulcs, nem üres-trim; ≤300), `TemplateLiteral` (a kifejezések `${…}`-re cserélve, csak ha marad nem-üres szöveg). Template: szöveg-csomópont (whitespace összevonva), statikus attribútumérték, `@esemény` név, valamint (Rust-port
bővítés) a `{{ … }}` interpolációk és a direktíva-kifejezések (`:attr="…"`, `@x="…"`, `v-if="…"`…) string-literáljai
(`'…'`, `"…"`, a `` `…` `` sablon statikus részei `${…}`-vel).

## comments
Babel `comments` — a `/** */` docblock kimarad; felvétel: `TODO|FIXME|XXX|HACK` kezdet (`tag`) vagy ≥15 kar. szöveg. `caller` = a legszűkebb tartalmazó metódus (`line < ln <= endLine`), különben `__top__`; `text` ≤200.

## Ismert sajátosságok / tesztelendő minták
- A Rust-portnak a Babel-viselkedést kell reprodukálnia (errorRecovery, `.vue` blokk-sorszámok). `.cjs`: `module.exports` nem ad metódust.
- `golden/samples/js/`: `types.ts` (interface/enum/generikus/osztály), `Badge.tsx`, `Plain.jsx`, `OptionsApi.vue`, `SetupTs.vue`, `legacy.cjs`, `reexport.mjs` (re-export nem kerül `imports`-ba), `helpers.js`.
- compare.py nem veti össze a JS-specifikus felső mezőket (`module`, `sfc`, `parseError`); a mezőn belüli extra kulcsok (`kind`, `args`, `on`, `resolved`, `category`, `handler`) a `clean()` miatt igen.
