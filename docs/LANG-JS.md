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

**Rust-port (3. fázis):** a megjelenített kulcs (`display`, `module`) a *projekt-gyökérhez* relatív, nem a csomag-gyökérhez (a fentiek `cfg.root`-ja): a legközelebbi `.git` / `.callgraph` mappa, ennek híján a csomag-gyökértől összefüggően felfelé a legfelső mappa workspace-jelzővel (`package.json`, `pnpm-workspace.yaml`, `tsconfig.json`, …), így egy fájl a monorepó minden csomagjából ugyanazon a kulcson áll. A `paths` a leghosszabb előtag szerint illeszt, minden célt kipróbál, a `*` lehet a minta közepén is. A tsconfig `extends` (relatív, lánc, tömb) és `references` (a hivatkozott config `paths`-a a sajátok után) is feloldódik; a `vite.config.*` / `vitest.config.*` `resolve.alias` (objektum vagy `[{find, replacement}]`; string, `fileURLToPath(new URL(..))`, `path.resolve(__dirname, ..)`) a `paths` után jön, csak létező célra. Kiterjesztés-próba: `.js .ts .vue .mjs .tsx .jsx .mts .cjs .cts`, `index.{js,ts,vue,tsx,jsx,mjs}`, és a TS-es `./a.js` → `a.ts`; mappánként egy (5 másodpercig érvényes) listázás az `is_file` hívások helyett. A pack formátum-verziója emelkedett (a régi tároló csendben újraépül).

## Hívás-kategóriák
`categorize` (104): forrás `vue` → `vue_api`; `@inertiajs/*` → `inertia`; `vue-i18n`/`laravel-vue-i18n` → `i18n_helper`; `ziggy-js` vagy `ziggy` a névben → `route_helper`; `axios` vagy `…AxiosHandler.js` → `http_call`; lokális és (`/Composable(s)/` az útban vagy `use[A-Z]…` név) → `composable_use`; `use[A-Z]…` bármi → `composable_use`; lokális → `module_use`; különben `package_use`.
`resolveCallee` (399–435) további: `internal_helper` (metódusnév / fájlon belüli függvény / `this.metódus`), `vue_macro` (VUE_MACROS, `resolved:'vue::<név>'`), `route_helper` (`route`, `resolved:'ziggy::route'`), `i18n_helper` (`t`,`$t`), `native_js` (NATIVE_GLOBALS), `global_helper` (ismeretlen azonosító), `external_call` (ismeretlen tag-hívás, `on:'<expr>'` nem azonosító gyökérnél), `vue_emit`, `vue_props`, `instantiation` (NewExpression, nem natív), `component_use` (template), `inertia` (`this.$inertia/$page`). A régi golden 6 `category` nélküli hívása (template `exprCalls` ágak) megszűnt: a template-kifejezések ugyanazon a feloldón mennek át, mint a script (lásd a Template bekezdést); csak a nem parsolható kifejezések tartaléka (regex) hagy `""` kategóriát a `toString` / `valueOf`-szerű fejeknél.

## Függvény-hivatkozás (`handler: true`, `(ref)`)
Hívás-/`new`-argumentumban, tömb-elemben és objektum-tulajdonság értékben álló azonosító vagy `ns.név`, ha a fájl saját
függvénye (`local_fns`), helyi import (kisbetűs, `module_use`), fájlon belüli objektum-literál metódusa, vagy
névtér-import (`import * as ns`) tagja: ugyanolyan `Call`, mint a hívás (`resolved`, `category`), de `handler = true`
— a kimenet `(ref)`-fel jelöli, a hívó-keresés és a `--depth` számolja (`Call::is_reference`). Csomag-, változó-,
osztály- és konstans-hivatkozás nem keletkezik. A `const x = computed(…)` kötés `x.value.….metódus` hívása `external_call`
(nem Vue API).

## Dinamikus hívás-jelölés (`dynamic_call`)
`obj[expr](…)` nem literál kulccsal (`type:"dynamic"`, `target` = `obj[expr]`, a régi `obj.[]` `external_call` helyett),
`new (expr)(…)` / `new obj[k]()`, `import(expr)` nem literál forrással (a literál `import('./x')` marad a régi `<expr>`
sor), `require(expr)` nem literál argumentummal (a `require` `native_js` hívás megmarad mellette). Nem hívó-él.
A Vue template-kifejezések ugyanazt a bejárót használják, ezért ott is jelöl (`@click="handlers[name]()"`); a nem parsolható kifejezés regex-tartaléka nem. Lásd `src/lang/js/calls/walk.rs`.

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
- **properties** `{container,name,line,type,readonly,static:false,kind:'prop'}` — `propsFrom` (116): `defineProps`/`props:` Options; objektum-alak (`type` = a `type` kulcs forrása ≤40, `required` → `readonly:true`; rövid alak: érték-forrás), tömb-alak (`type:'mixed'`), TS típus-literál / interface-törzs (`readonly` = `!optional`). `defineProps<Props>()` hivatkozott típusnévnél a fájlon belüli `interface` / `type` vagy az importált (relatív / alias-os import, `export interface`) típus tagjai; az importált tagok a `defineProps` sorára kerülnek, a csomagból jövő típus nincs feloldva (lásd `golden/samples/js/SetupTs.vue`).
- **constants** `{container,name,line,value,kind}`: `const` (top-level `UPPER_SNAKE` név, literál/tömb/objektum init, érték ≤80 kar.); `enum_case` (`Enum.Tag`, init-forrás vagy `null`); `emit` (`emit:<név>`, `value:null`, `emitsFrom` 139: tömb, objektum, TS hívás-szignatúra regex `(e: 'x'` és `'x': [` regex).

## calls (`walk` 436–526 + analyzeTemplate)
Alak: `{container,caller,line,type,target,on?,category,resolved?,args?,handler?}`; `type` = `func_call` / `method_call` / `new` / `component`. `caller` = a legközelebbi metódus-ős (`methodOf`), különben `setup`/`__module__`. `args` = az első ≤4 argumentum forrása ≤60 kar. `VariableDeclarator`-ban `const x = hívás()` / `await hívás()` / destrukturálás a `bindings`-be kerül (`resolved:'<callee>()'`, `.kulcs`; `t` → `i18n_helper`; `defineEmits` → `vue_emit`, `defineProps|withDefaults` → `vue_props`). Literál első argumentum: `vue_emit` → `resolved:'<Fájl>.vue::emit:<n>'` (.vue-n kívül `vue::emit:<n>`), `vue::provide` / `vue::inject` → `vue::provide:<kulcs>` (sztring vagy azonosító), `i18n_helper` → `i18n::<kulcs>`, `route_helper` → `route::<név>`. Tag-hívás: gyökér azonosító → `target` = `gyökér.rész.rész` (ha binding/natív) vagy az utolsó rész (`external_call`, `on`=gyökér); `this.x` → `internal_helper`/`inertia`/`i18n`/`vue_emit`/`external_call` (`target:'this.…'`).
**Fogadó-típus (TS, `calls/receivers.rs`)**: a `x.m()` / `this.x.m()` / `x!.m()` hívás `resolved` = `Típus::m`, ha a fogadó típusa egyértelmű: típusos paraméter (`(repo: Repo)`, a konstruktor paraméter-property-je `constructor(private repo: Repo)` is), típusos osztály-property (`private repo: Repo`, `repo!: Repo`), `this.repo = new Repo()` / `repo = new Repo()` property-érték, `const x: Repo = …`, `const x = new Repo(…)`. A típusnév a fájl top-level osztálya / interfésze (`category:'internal_helper'`) vagy importált név (az import exportált neve, `import { Repo as R }` → `Repo`; `import * as m` → `m.Repo` → `Repo`; a kategória az importé, pl. `module_use` / `package_use`). Egy név a függvényben (osztályban) csak akkor típusos, ha minden kötése ugyanazt adja (az annotáció dönt; különben a `new` értékadások egyezése); a típus nélkül deklarált belső név (pl. `(r) => r.m()`) árnyékolja a külsőt. Feloldatlan marad: generikus paraméter, `any`/`unknown`, unió (`Repo | null`), nem importált / nem a fájlban deklarált típus (`Map`, `HTMLElement`), mélyebb lánc (`this.a.b.m()`), a nem-metódus `function`-ön belüli `this`, Options API `this`. JS-ben nincs `?::m` (a Rusttal ellentétben): a tömb-/DOM-metódusok miatt a `Típus::m` keresés túl zajos lenne. A `this.m()` saját osztálymetódusra `resolved:'Osztály::m'` (`internal_helper`).
`scanFns` (378–397) előre gyűjti a fájlon belül definiált függvényneveket (`localFns`) és az `{emit}` destrukturált paramétert (`vue_emit` binding).
**Template** (`analyzeTemplate`, `caller:'template'`): elem `Pascal`/importált tag → `component_use` (`type:'component'`, `resolved` az import). A direktíva-kifejezések (`{{ }}`, `:x`, `@x`, `v-if`/`v-show`, `v-model`, a `v-for` forrása) az oxc-vel parsolódnak (`(kif\n);`; a `@x` kezelő, ha így nem megy, utasítás-listaként: `a(); b++`), és ugyanaz a `Walker` + `Scope` járja be őket, mint a scriptet (`template/script.rs`); a `Scope` a `<script>` + `<script setup>` nevei és kötései (+ a metódusnevek, Options API `methods`/`computed` is). Így a kategória és a `resolved` a scripttel azonos: kötésből (`const store = useX()` → `store.send()` = `…useX().send`), importból (`package_use`, `composable_use`, `http_call`, `inertia`…), `native_js` (`Math.max`), `global_helper` az ismeretlen név, `external_call` az ismeretlen tag-hívás (`target` az utolsó tag, `on` a gyökér, mint a scriptben). `$t`/`t`/`$tc` → `i18n_helper` (`i18n::<kulcs>`), `route` → `route_helper` (`route::<név>`), `$emit` / kötetlen `emit` → `vue_emit` (`<Fájl>.vue::emit:<név>`). A `v-for` elemnevei (`(item, i) in list`, destrukturálás) és a slot-propok (`#x="{ a, b }"`, `v-slot`, `slot-scope`) az elem többi direktívájában és a gyerekeiben helyi nevek: hívásuk (`item.open()`) nem keletkezik. A `v-for` forrása a `in`/`of` utáni kifejezés, a slot-érték minta (nem hívás). Ha a kifejezés nem parsolható (hibás / Vue-specifikus szintaxis), arra az egy kifejezésre a régi regex (`expr_calls`, név szerinti kategória) fut. `@event` → `strings` `@<esemény>`; puszta metódusnév-kezelő (`@click="save"`) → `internal_helper` + `handler:true` (hivatkozás, nem hívás).

## controlFlow
`{container,caller,type,line,cond,branches[{label,startLine,endLine,fallThrough?}]}`. `if` (az `else if` lánc a külső if-nél, ágak: `if (…)`, `elseif (…)`, `else`), `switch` (`case …`/`default`, `fallThrough` üres case-re). Template: `v-if` / `v-else-if` (`cond` ≤100, ág-címke `v-if="…"` ≤60, sor = az elem sortartománya). Más vezérlés (for, ternary, try) nincs.

## strings
`{container,caller,line,value}`: `StringLiteral` (nem import/export-forrás, nem objektum-kulcs, nem üres-trim; ≤300), `TemplateLiteral` (a kifejezések `${…}`-re cserélve, csak ha marad nem-üres szöveg). Template: szöveg-csomópont (whitespace összevonva), statikus attribútumérték, `@esemény` név, valamint (Rust-port
bővítés) a `{{ … }}` interpolációk és a direktíva-kifejezések (`:attr="…"`, `@x="…"`, `v-if="…"`…) string-literáljai
(`'…'`, `"…"`, a `` `…` `` sablon statikus részei `${…}`-vel).

## comments
Babel `comments` — a `/** */` docblock kimarad; felvétel: `TODO|FIXME|XXX|HACK` kezdet (`tag`) vagy ≥15 kar. szöveg. `caller` = a legszűkebb tartalmazó metódus (`line < ln <= endLine`), különben `__top__`; `text` ≤200.

## JSX (`.js`, `.jsx`, `.tsx`)
- A Rust-oldali oxc-parse a `.js`, `.mjs`, `.cjs`, `.jsx` és `.tsx` fájlt JSX-képesként olvassa (CRA / babel / vite
  projektek sima `.js`-ben is írnak JSX-et); a `.ts` / `.mts` / `.cts` nem JSX-képes (ott a `<T>x` típus-állítás
  érvényes). A `.vue` `<script>` kezelése változatlan (a `lang` attribútum dönt).
- Komponens-használat = hívás: a nagybetűs JSX-elem nyitó címkéje (`<Button />`, `<Button>…</Button>`, tagolt
  `<Ui.Card.Head />`) `Call`, `kind: "component"`, `category: "component_use"` (mint a Vue-sablonban); a cél a név
  (tagolt névnél `Ui.Card.Head`), a `resolved` az importból, mint a függvényhívásnál (`<import>::<név>`; névtér-importnál
  `<import>::Ui.Card.Head`). A fájlon belül definiált komponens `internal_helper`, feloldás nélkül; az ismeretlen
  (globális) nagybetűs név feloldatlan `component_use`. Nem hívás: a kisbetűs elem (`<div>`), a fragment (`<>`), a
  névtér-es név (`<a:b>`), a `this`-gyökerű név és a záró címke. A prop-ok hívásai (`onClick={() => go()}`) a
  szokásos módon hívásként jelennek meg.
- Függvény-komponens props (`lang/js/react/`, `ReactPass`): a `.jsx` / `.tsx` / `.js` / `.mjs` fájl nagybetűs, JSX-et
  tartalmazó top-level függvénye (`function X`, `const X = () =>`, `memo(…)` / `forwardRef(…)` / `observer(…)` burkoló,
  `export default`). A props az első paraméterből: destrukturálás (alapérték = nem kötelező, `...rest` = nyitott),
  TS típus (inline, helyi vagy importált `interface` / `type`, `extends`, `A & B`, `PropsWithChildren<T>`; a
  `props: Props`, a `FC<Props>` változó-annotáció és a `forwardRef<El, Props>` / `memo<Props>` típus-argumentum),
  `X.propTypes = { a: PropTypes.string.isRequired }`, `X.defaultProps`, és típus nélküli `props` objektumnál a
  `props.x` olvasások (ilyenkor nyitott). A nem feloldható típus (`Omit<…>`, `*HTMLAttributes`, `ComponentProps`,
  csomagból jövő, unió) nyitottá teszi a halmazt. A tények a fő container konstansai: `react_comp` (`X`, érték:
  `default`, `export:<név>`, `open`) és `react_prop` (`X.title`, érték `*` = kötelező); az outline a komponens alatt
  `props (N): title*, size, …` sorban mutatja, a `--format props` (`packages/vuecontract/react.rs`) a JSX-használatokat
  veti össze velük (hiányzó kötelező, ismeretlen; `{...spread}` a „hiányzó”-t, a nyitott halmaz az „ismeretlen”-t
  némítja; `key` / `ref` / `children` / `className` / `style` / `data-*` / `aria-*` sosem ismeretlen). Csak a
  statikusan kötött komponens (fájlon belüli, vagy projekt-fájl importja, amelynek exportja maga a komponens) számít.
- Hook és context: a `use[A-Z]…` import `composable_use` (a projekt hookja a fájlra oldódik: `src/hooks/useCart.ts::useCart`);
  az outline a saját React-hookot `[hook]`-kal jelöli. A `createContext`, a `<X.Provider>` / `<X.Consumer>` és a
  `useContext(X)` egy kulcson áll: `react::context:<definiáló fájl>::X`.
- react-router (`webroutes`): `<Route path element|Component index>` fa, `createBrowserRouter` / `createHashRouter` /
  `createMemoryRouter` / `useRoutes` tömb (`element`, `Component`, `lazy`, `children`); a literális `<Link to>` /
  `<NavLink>` / `<Navigate>` / `navigate('/x')` útvonal a táblához illesztve (`:param` egy szegmens, `*` a maradék),
  találat nélkül `⚠ unknown route path`; tábla nélkül, nem literális route-útvonalnál vagy `addRoute` mellett nincs
  figyelmeztetés.
- Inertia + React: a page-fájl `.tsx` / `.jsx` is lehet; a page props-ai az alapértelmezetten exportált komponens
  props-ai (a „never passed” ellenőrzés a kötelezőkre), a `trace` a page `.tsx` / `.jsx` importjait (nagybetűs alias)
  `uses` sorban adja. A cache formátum-verziója 14.

## Route-táblák (`--format routes`, `packages/webroutes/`)
Nem a gráfból, hanem a forrásszövegből (`core::srcscan`: megjegyzés-tisztítás, zárójel-párosítás, argumentum-vágás)
olvas, egy lekérdezésenként, csak a `routes` / `ziggy` nézetnél, és csak ha a lekérdezett sorok között van JS / TS / Vue
fájl; az eredmény a Laravel route-sorok után kerül a kimenetbe (a régi sztring-alapú `path "/x"` sorok helyett).
- **Kliens**: `routes: [` / `const routes: RouteRecordRaw[] = [` / `export default [` tömb `{ path, name, component,
  redirect, children }` objektumokkal. A gyerek útja a szülőé után fűzve (abszolút gyerek-út önálló); a komponens
  `() => import('…')` vagy importált azonosító, a fájl a projekt gyökeréhez képest (`@/` / `~/` = a legközelebbi `src`);
  `redirect '/x'` → `redirect /x`, `{ name }` → `redirect name:x`. Nevek használata: `router|$router|useRouter().push|replace|
  resolve({ name: 'x' })`, `<RouterLink|router-link :to="{ name: 'x' }">`, `to: { name: 'x' }`; csak literál név,
  csak ha van route-tábla, és nincs `router.addRoute(` a projektben.
- **Szerver**: ha a fájl `hono` / `express` / `itty-router` / `fastify` / koa / elysia importot tartalmaz, a
  `x.get|post|put|patch|delete|options|head|all('/út', …, kezelő)` (az utolsó argumentum a kezelő; szám / sztring /
  objektum nem kezelő; `).get(` lánc az előző fogadót örökli) és az `x.route|use('/előtag', alRouter)` (az
  előtag az al-router sztring-útjaira kerül; importált al-routernél a forrásfájl összes útjára). Importtól
  függetlenül: `név('GET', /regex/ | '/út', kezelő)`. A kezelő: azonosító → importból `fájl::függvény` (névtér-importnál
  `ns.fn`), fájlon belüli deklaráció → `fájl::név`, `wrapper(kezelő, …)` → az első argumentum, névtelen függvény →
  `fájl:sor`. Feloldatlan alias-import marad úgy, ahogy írva van.
- **Kliens-hívás ↔ szerver-route**: `fetch|ofetch|$fetch|useFetch|axios.<ige>|http.<ige>|new WebSocket|new EventSource`
  első argumentuma, ha literállal kezdődő `/…` út (a `${}` és a nem literál `+` tag `:param`, a query le van vágva);
  a metódus az `axios.<ige>` / `fetch` `method:` literálja (opciók nélkül GET, nem literál: bármi). Az illesztés a
  teljes útra és a metódusra szól (a `:param` helyén mintaérték), regex-route-nál a regexre.
  „Nincs szerver-route” sor: csak ha a lekérdezett fájlokban van szerver-route, és csak az olyan kliens-URL, amelynek
  első szegmense egyezik valamelyik szerver-route-éval.
- Ismert hiányok: a route-útra nincs külön `--grep` kulcs (a kliens-literált a sztring-keresés találja, a
  szerver-route-ot a nézet mutatja); `app.route('/x').get(h)` lánc, `app.on(...)`, `${BASE}/…` kezdetű kliens-URL és tsconfig-alias importú
  kezelő nincs feloldva; az elemzés idejű (`FileGraph`) tényekké emelés (R4) még nincs meg.

## Vue komponens-szerződés (`--format props`)
- **Emit-él**: az emit-kulcs komponens-hatókörű, `<Fájl>.vue::emit:<név>` (`emit('x')`, `$emit('x')`, `this.$emit('x')`,
  `{ emit }` paraméter). A szülő `@x="h"` / `v-on:x` figyelője egy importált `.vue` komponensen (`<CellMenu @remove>`) ugyanerre
  a kulcsra mutat (`vue_emit`, `target:'@x'`; kebab-case → camelCase; a natív DOM-események — `click`, `input`, … — és a
  dinamikus `@[ev]` nem kapnak élt). Így a `--grep 'CellMenu.vue::emit:remove$'` az emittáló függvényt és a szülőt is mutatja.
  Azonos nevű `.vue` fájl két mappában ugyanazt a kulcsot adja.
- **Outline**: `slots (N)` (a `<slot>` / `<slot name>` kimenetek; konstans `slot:<név>`), `exposes (N)` (`defineExpose({…})`,
  konstans `expose:<név>`). `provide` / `inject`: egy kulcs `vue::provide:<kulcs>` mindkét oldalra.
- **Pinia**: `const useX = defineStore('id', …)` → `store` container; options-stílusban az `actions` / `getters` metódusok
  (`kind: action` / `getter`), a `state` kulcsai tulajdonságok; setup-stílusban a visszaadott függvények / `computed`-ok / nevek.
  A `useXStore().akció()` és a `const s = useXStore(); s.akció()` is `<fájl>::useXStore().akció` él.
- **`--format props`**: nem a gráfból, hanem a lekérdezett `.vue` fájlok sablonjaiból (`template_facts`) és a gyerek elemzett
  gráfjából (props, `model:*`) számol; a projekt összes `.vue` fájlját beolvassa. A gyerek: a szülő `.vue` importja (a megjelenített
  import-út egyetlen fájlra illeszkedik), vagy — `unplugin-vue-components` a `package.json`-ban / `components.d.ts` — az egyetlen azonos
  nevű `.vue` fájl (`(auto)`). Hiányzó kötelező prop: `required: true` / nem opcionális TS-tag `withDefaults` érték nélkül, ha a szülő
  nem ad `v-bind="obj"`-ot és nem adja át (kebab = camel, `v-model` / `v-model:x` átadja). Ismeretlen prop: csak ha a gyerek propjai
  teljesen ismertek (inline `{…}`, helyi `interface` / `type` literál `extends` / `&` nélkül, futásidejű objektum / tömb spread nélkül;
  `$attrs`, `useAttrs`, `inheritAttrs`, `mixins`, `extends`, importált vagy összetett típus → soha), és a név nem szabványos HTML-attribútum /
  `data-*` / `aria-*` / figyelő. Ismeretlen slot: a szülő `#x` / `v-slot:x` neve, ha a gyerek sablonjában a `<slot>`-ok statikusan
  ismertek (nincs `$slots`, `useSlots`, `defineSlots`, render-függvény, dinamikus slot-név).

## Ismert sajátosságok / tesztelendő minták
- A Rust-portnak a Babel-viselkedést kell reprodukálnia (errorRecovery, `.vue` blokk-sorszámok). `.cjs`: `module.exports` nem ad metódust.
- `golden/samples/js/`: `types.ts` (interface/enum/generikus/osztály), `Badge.tsx`, `Plain.jsx`, `OptionsApi.vue`, `SetupTs.vue`, `legacy.cjs`, `reexport.mjs` (re-export nem kerül `imports`-ba), `helpers.js`.
- compare.py nem veti össze a JS-specifikus felső mezőket (`module`, `sfc`, `parseError`); a mezőn belüli extra kulcsok (`kind`, `args`, `on`, `resolved`, `category`, `handler`) a `clean()` miatt igen.
