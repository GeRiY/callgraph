//! A Vue / JS / TS célnyelv a kódgenerátorhoz (`callgraph gen`): `.vue` egyfájlos komponens (script setup, template,
//! style) és composable-fájl (`useX.ts` / `.js`). A PHP-generátorral közös a tokenizáló / fa (`lexer`, `parser`:
//! `>` gyerek, `+` testvér, `^` fel, `*n` ismétlés, `$` számláló, `( )` csoport, `{ … }` nyers törzs, `[ … ]` lista)
//! és a parancsok (`WriteNew` / `InsertInto` / `ReplaceRange`, mentés, `undo`); a nyelvi rész itt él, a PHP-t
//! érintetlenül hagyva. Az `Emitter` trait PHP-alakú (típus-deklarációk), ezért a Vue-oldal külön emittereket
//! használ: `script::ScriptEmitter`, `template`, `style`; az összeállítás `builder` (Builder minta).
//!
//! - **Interpreter** — `parse`: a közös fából `model::Program` (script-tagok, template-elemek, stílus-szabályok);
//! - **Strategy** — `script` (TS / JS, `;` van / nincs), `template`, `style` emitterek;
//! - **Builder** — `builder::SfcBuilder`, `builder::composable_file`;
//! - **Facade** — `generator::VueGenerator`: kifejezés → fájl → ellenőrzés → parancs; a CLI a fájl kiterjesztése
//!   (vagy a kifejezés feje) alapján választ a PHP-s `Generator` és ez között (`wants_vue`).
//!
//! # Nyelvtan
//! Fejek: `vue:Név` (SFC; a gyerekei a szakaszok: `vue:Card>script>props(title:str)^template>h1{{ title }}^style>.a{x:y}`,
//! a `^` lép ki a szakaszból), `comp:useCart` (composable-fájl; a gyerekei a tagok);
//! szakaszok: `script>…`, `template>…`, `style>…` (alapból scoped; `style!` / `style:scoped` scoped,
//! `style:global` nem). Fej nélküli szakaszok / csupasz tagok az `--into` / `--replace` célra.
//!
//! Script-tagok:
//! - `props(title:str,items:list<Item>,size?:int=3)` — `?` vagy `=érték`: nem kötelező;
//! - `emits(save:int,close)` — a `név:típus` a payload;
//! - `ref:count:int=0`, `ref:items:list<Item>=[]` (érték nélkül a típus alapértéke: `0`, `''`, `false`, `[]`, `null`);
//! - `computed:total:int{ items.value.length }`; `fn:submit(e:Event):void{ … }`, `async:load():list<Item>{ … }`;
//! - `form(name,email,password)` / `form:login(email,remember:bool=true)` → `const form = useForm({…})`
//!   (`@inertiajs/vue3`); `const:API:str='/api'`, `let:n:int=0`; `on:mounted{ … }` → `onMounted(() => { … })`;
//! - `use:@/Components/Card` (default import a fájlnévvel; PascalCase fájl → `.vue`), `use:path as Név`,
//!   `use:vue(ref,computed)` és `use:{useX}@/composables/useX` / `use:{a,b as c}@/út` (nevesített import).
//!   Kapcsos zárójel nélkül a `@/…` út célfájlját (`resources/js` / `src`; `.ts`, `.js`, `/index.ts`, `/index.js`)
//!   beolvassuk: ha nincs `export default`-ja, de van a fájlnévvel azonos nevesített exportja (`export function
//!   useX`), nevesített import lesz; különben default. A `ref` / `computed` / `onX` / `useForm` importja automatikus;
//!   a `defineProps` / `defineEmits` fordítói makró, nem kell import;
//! - `type:Flag{flag:str,on?:bool,tags:list<str>}` → `interface Flag { … }` (composable-fájlban a függvény előtt
//!   `export interface`; JS-ben JSDoc `@typedef`); a név props / ref / fn típusában használható (`list<Flag>`);
//!   inline objektum-típus bárhol: `list<{ flag: str; on?: bool }>` → `{ flag: string; on?: boolean }[]`;
//! - `raw{ const { a, b } = useX() }` (szó szerint kiírt utasítások a tagok sorrendjében) és a rövid
//!   `const:{ a, b }=useX(items)` → `const { a, b } = useX(items);` (a nevek deklaráltnak számítanak az `--into`
//!   ütközés-vizsgálatnál; a composable `return`-jébe nem kerülnek).
//!
//! Template (Emmet): `div.card#main>h1{{ title }}+ul>li[v-for="item in items" :key=item.id]{{ item.name }}`,
//! `button[@click=submit type=button]{Save}`, `Card[:title=title]` (PascalCase = komponens, `@/…/Név.vue` importja
//! automatikus, ha a projektben van ilyen fájl; `Link` / `Head` az Inertiából), szöveg `{…}` (az aposztróf szabad),
//! mustache `{{ … }}` (több mustache egy szövegben: `{a {{ x }} b {{ y }}}`; a `span{{ x }}` rövidítés és a
//! `span{{{ x }}}` / `p{{{ a }} / {{ b }}}` alak is `{{ … }}`-t ad: a `{{`-vel kezdődő szövegtörzset nem vesszük körül
//! újabb párral), `li.item$*3` (`$` = számláló).
//! Gyerek nélküli komponens / void elem önzáró; az attribútum-érték idézőjel nélkül is írható.
//!
//! Style: `style>.card{padding:1rem;gap:.5rem}+.card h1{margin:0}` (a szelektor szóközt is tartalmazhat; a `>` /
//! `+` kombinátor nem — azt a törzsbe írt beágyazott blokkal: `.a{ > li { … } }`).
//!
//! Composable: `comp:useCart>ref:items:list<Item>=[]+computed:total:int{ items.value.length }+fn:add(item:Item):void{ … }`
//! → `export function useCart() { …; return { items, total, add }; }`.
//!
//! # Általános JS / TS modul (`mod:`, `class:`)
//! `mod:strings>type:Opts{sep?:str}+fn:join(p:list<str>,o?:Opts):str{ … }+const:LIMIT:int=10+-fn:clean(s:str):str{ … }`
//! — a `.ts` / `.js` / `.mjs` / … fájl a kiterjesztés szerinti nyelven (TS: típusok; JS: JSDoc `@param` / `@returns` /
//! `@typedef`, típus nélküli kód); cél nélkül (`--dry`) a projekt stílusa. A `--write x.ts` csupasz tagjai is modult
//! adnak (kivéve, ha `ref:` / `computed:` / `props` van köztük: az `comp:` composable marad).
//! - tagok: `fn:` / `async:` → `export function` / `export async function`; `const:` / `let:` → `export const`;
//!   `type:Név{…}` → `export interface` (JS: `@typedef`); `type:Név=A|B` → `export type Név = A | B;` (idézőjeles
//!   literál is: `type:Mode='a'|'b'`); `class:` (lent); `use:` és `raw{…}` mint máshol;
//! - export: alapból exportált; `-fn:x` (előtag `-`) modul-privát; `!fn:x` / `!class:X` / `!const:X` → `export default`;
//! - osztály: `class:Név<Ős>[A,B]>tagok` (`class~:` absztrakt); a tagok a PHP-s nyelvtant követik: `+` public, `-` private,
//!   `#` protected, majd `::` static, `~` abstract, `%` readonly; `$mező:típus=érték`, `ctor(-dep:Repo,+n:int=3)` (TS:
//!   `private readonly dep: Repo` / `public n: number = 3` paraméter-property; JS: `this.dep = dep;`),
//!   `+név(p:T):R{…}` / `async:név():R{…}`; JS-ben a láthatóság nincs kiírva, a típusok JSDoc-ban;
//! - szerkesztés: `--into x.ts` a tagokat a fájl végére (vagy `--after név` után) teszi, az importokat egyesíti, a már
//!   deklarált nevet visszautasítja; `--into x.ts::Osztály [--after tag]` osztálytagot szúr be (az oxc-span szerint);
//!   `--replace x.ts::név` / `x.ts::Osztály::tag` / `x.ts:La-Lb` cserél (a sima tag megtartja az eredeti `export`-ot).
//!
//! Nyelv / stílus: TypeScript (`<script setup lang="ts">`, típusos `defineProps<{…}>()`, `ref<T>()`), ha a projektben
//! van `tsconfig.json` vagy valamelyik `.vue` fájl `lang="ts"`-t használ (`.ts` / `.js` fájlnál a kiterjesztés dönt;
//! meglévő fájlnál a fájl); egyébként sima JS (`defineProps({ title: { type: String, required: true } })`). A
//! pontosvessző a meglévő fájlok többségét követi (alapból van), aposztróf, 2 szóköz.
//!
//! Szerkesztés: `--into x.vue` + `script>…` (a `</script>` elé, vagy `--after <név>` a függvény / const után),
//! `template>…` (`--after L<n>` után; nélküle a gyökérelem záró tagje elé), `style>…` (a meglévő blokk végére;
//! hiányzó blokkot létrehoz); a szükséges importokat a meglévő import-sorokba egyesíti. `--into useX.ts` a tagokat a
//! `return {` elé teszi és a visszaadott objektumot bővíti. `--replace f.vue:L10-L20` / `f.vue::név` mint a PHP-nál.
//! Írás előtt a teljes szöveget ellenőrizzük (script / composable: oxc; template: tag-egyensúly; style: CSS-parser).

mod builder;
mod check;
mod edit;
mod generator;
mod imports;
mod locate;
mod model;
mod parse;
mod project;
mod script;
mod style;
mod template;
mod types;

#[cfg(test)]
mod tests;

pub use generator::{VueGenerator, wants_vue};

/// A projekt Vue-stílusa a preset számára: TypeScript-e, pontosvesszős-e, és a behúzás-egység.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectFacts {
    pub ts: bool,
    pub semi: bool,
    pub unit: usize,
}

/// A projekt kódstílusa és behúzása (a meglévő `.vue` fájlokból).
pub fn project_facts(env: crate::generate::Env<'_>) -> ProjectFacts {
    let project = project::VueProject::new(env.fs, env.root);
    let style = project.code_style();
    ProjectFacts {
        ts: style.ts,
        semi: style.semi,
        unit: project.indent(),
    }
}

/// A `<név>.vue` komponens import-útja (`@/Components/Név.vue`), ha a projektben van ilyen fájl.
pub fn component_path(env: crate::generate::Env<'_>, name: &str) -> Option<String> {
    project::VueProject::new(env.fs, env.root).find_component(name)
}

/// Egy kész `.vue` szöveg ellenőrzése (script: oxc, template: tag-egyensúly, style: CSS-parser); hiba: egy sor.
pub fn check_sfc(text: &str) -> Result<(), String> {
    check::check_vue(text).map_err(|p| format!("{} L{}: {}", p.section, p.line, p.message))
}

/// Egy kész `.js` / `.ts` fájl szintaxis-ellenőrzése (oxc); hiba: egy sor.
pub fn check_script(rel: &str, text: &str) -> Result<(), String> {
    check::check_script_file(rel, text).map_err(|p| format!("L{}: {}", p.line, p.message))
}
