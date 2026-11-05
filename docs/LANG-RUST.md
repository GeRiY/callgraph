# LANG-RUST — a Rust-leképezés (`.rs`)

Modul: `src/lang/rs/` (`RustBackend`, szerep: backend, azonosító: `rust`, kiterjesztés: `rs`). A modul neve `rs`, nem
`rust`: a fájlkiterjesztést követi, és nem keveredik a `rust` toolchain- / std-elnevezésekkel.
Parser: `syn = "=2.0.119"` (`full`, `parsing`, `printing`, `visit`) + `proc-macro2 = "=1.0.107"` (`span-locations` a
sorszámokhoz és a pontos forrás-szövegekhez). Tiszta Rust, nincs C-toolchain (musl / Windows / macOS build egyszerű).
A syn-típusok csak a `parser/` (Adapter: `SynAdapter`, `RustParser`) és a `visitor/` alatt jelennek meg.
Minták: `golden/samples/rust/` (saját `Cargo.toml` axum-mal, `src/{handlers,shapes,state,aliases}.rs`).

## Csővezeték
`SynAdapter::parse` (`syn::parse_file`; utána `proc_macro2::extra::invalidate_current_thread_spans()`, hogy a szálankénti
forrás-térkép ne nőjön) → `visitor::run`: egyszer elkészül a `Scopes` (minden elem a végleges, minősített nevével),
majd sorban: `ImportsVisitor` → `SymbolsVisitor` (névtábla) → `StructureVisitor` → `CallsVisitor` → `ControlFlowVisitor`
→ `StringsVisitor` → `CommentsVisitor`. Szintaktikai hibánál (a syn nem hibatűrő) csak a fájl-container készül el,
a hiba a `warnings`-ba kerül (`<út>: syntax error (line N): …`, magyarul `szintaktikai hiba`).

## Fájl-szintű keretek
- **framework**: a fájl mappájától felfelé az első `Cargo.toml` függőségei (`[dependencies]`, `[dev-dependencies]`,
  `[build-dependencies]`, `[workspace.dependencies]`, `[target.….dependencies]`, `[dependencies.<név>]`) közül az első
  ismert, prioritás szerint: bevy, tauri, leptos, dioxus, yew, axum, actix-web, rocket, warp, poem, diesel, sqlx,
  sea-orm, clap, tokio, async-std, serde (`framework/known.rs`; könyvtáranként cache-elve). Nincs találat: `null`.
- **namespace**: mindig `null` (a modul-út a fájl útjából következik, lásd lent). **writes**: mindig `[]`.
- **Modul-út** (`module_path.rs`): az utolsó `src` mappa utáni rész (`src/cli/args.rs` → `crate::cli::args`;
  `mod.rs` → a mappa; `lib.rs` / `main.rs` → crate-gyökér). `src` nélkül a fájlnév (`tests/x.rs` → `x`).

## containers
| Rust | `type` | `name` | megjegyzés |
|---|---|---|---|
| a fájl | `module` | a modul-út utolsó szegmense (`args`), gyökérnél `crate` | `line 1`, `endLine` = sorok száma, `doc` = `//!` |
| `mod x { … }` | `mod` | `x` (beágyazva `a::b`) | a `mod x;` deklaráció nem container |
| `struct`, `enum`, `union` | ugyanaz | a név, inline modulban minősítve (`tests::Stub`) | |
| `trait` | `trait` | | `extends` = a supertraitek (`Send`, `fmt::Debug`) |
| `impl X` / `impl T for X` | — | | a metódusok X-hez kerülnek; `implements` = a traitek (`Display`, `From<io::Error>`) |
| `impl T for Idegen` | `impl` | a céltípus | ha a típus nem ebben a fájlban van; több blokk egy containerbe olvad |

A `line` a láthatóság (`pub`) vagy a kulcsszó sora (az attribútumok és a doc-komment nélkül), az `endLine` a záró `}`.

## methods
`{container, name, line, endLine, visibility, static, abstract, kind, params, returnType, doc, body:null}`
- Szabad `fn` → a fájl- / inline-modul containere; `impl`- és trait-metódus → a típus / trait.
- **visibility**: `pub`, `pub(crate)`, `pub(super)`, `pub(in …)`, különben `private`; trait-metódus és trait-implementáció: `pub`.
- **kind**: `fn` (szabad), `method` (van `self`), `assoc` (társított, `self` nélkül); aszinkronnál `async ` előtag.
- **static**: társított függvény `self` nélkül. **abstract**: trait-metódus alapértelmezett törzs nélkül.
- **params**: a paraméterek forrása egy sorba vonva (`&self`, `&mut self`, `id: u64`), ≤60 karakter. **returnType**: a `->` utáni típus.
- **doc**: a `///` doc-komment első bekezdése egy sorba vonva (≤300).
- `macro_rules! x` → `x!` (`kind: macro_rules`, `#[macro_export]` → `pub`).
- Név-ütközés containeren belül (pl. két `impl` blokk azonos nevű metódussal) → `<név>@L<sor>`.

## properties / constants
- **properties**: struct- / union-mezők; tuple structnál a név az index (`0`, `1`). `type` = a típus forrása,
  `visibility` mint fent, `static` / `readonly` mindig `false`, `doc` = `///`.
- **constants**: `const` (`kind: const`, az `impl`- és trait-konstansok is), `static` / `static mut`, `type` alias
  (`kind: type`, `value` = a típus), enum-variáns (`kind: enum_case`, `value` = a diszkrimináns, ha van). `value` ≤80.

## imports
A fájl minden `use`-fája kilapítva (a függvényeken és inline modulokon belüliek is), plusz `extern crate`:
`use a::{b, c as d}` → `b → a::b`, `d → a::c`; `use a::{self}` → `a → a`; glob: `a::* → a::*`; `as _` kimarad.
A kimenet a leírt utat adja (`super::X`); a névtáblába a kanonikus kerül (`crate::cli::X`).

## calls
`{container, caller, line, type, target, resolved?, on?, category}`; `caller` = a függvény neve, a konstans-értékekben és
a felső szintű makróhívásokban (`thread_local! {…}`) `__top__`.
- **type**: `func_call` (`f()`, `std::fs::read()`), `static_call` (`Foo::new()`, `Self::x()` — az utolsó előtti szegmens
  nagybetűs), `method_call` (`x.bar()`, `on` = a fogadó forrása ≤40), `macro` (`println!`).
- **target**: a leírt alak (`Foo::new`, `bar`, `vec!`); **resolved**: a feloldott név, ha eltér.
- **Makró-argumentumok**: a makró nincs kibontva, de a törzse vesszős kifejezés-listaként (`format!("{}", f(x))`), vagy
  utasítás-sorozatként (`vec![0; n]`, `thread_local! { static X: … = f(); }`) parse-olva bejárásra kerül (hívások,
  stringek, elágazások). Ha egyik sem sikerül (pl. `json!({…})`, `tokio::select!`), a törzs átlátszatlan; ilyenkor csak a
  string-literálok kerülnek ki a tokenekből.

### Kategóriák (`categorize/rules.rs`, `core::Chain`, az első illeszkedő dönt)
| Kategória | Mikor | `resolved` |
|---|---|---|
| `macro_use` | minden makróhívás | import szerinti teljes út (`tracing::info!`) |
| `external_call` | helyi változó / paraméter / closure hívása (`cb(x)`) | — |
| `internal_helper` | `self.x()` a container saját metódusára; `Self::x()`; a fájl saját típusa / függvénye / enum-variánsa; metódusnév, ami pontosan egy saját containerben létezik | `Típus::név`, `modul::fn` |
| `native_rust` | std / core / alloc út vagy ilyen import; prelude, primitív és gyakori std-név import nélkül (`String`, `Some`, `HashMap`, `u32`); gyakori std-metódus (`unwrap`, `iter`, `to_string` — `categorize/data.rs`) | a teljes út |
| `framework_use` | ismert framework crate (axum, tower, hyper, actix_web, rocket, tokio, serde, serde_json, clap, diesel, sqlx, bevy…) | a teljes út |
| `module_use` | a crate saját eleme (`crate::` / `super::` / `self::` út vagy ilyen import), illetve feloldhatatlan név (glob-import, szülő-modul) | a rövid kulcs: az utolsó két szegmens (`args::parse`, `Method::new`) |
| `package_use` | más crate (külső függőség) | a teljes út (`regex::Regex::new`) |
| `external_call` (tartalék) | ismeretlen fogadójú metódushívás | — |

A `module_use` rövid kulcsa szándékosan egyezik a másik fájl container-nevével (`crate::cli::args::parse` → `args::parse`,
a `src/cli/args.rs` fájl-containere `args`), így a `callers --depth` fájlokon át is lánc-ol.

## controlFlow
`if` / `else if` / `else` lánc egy sorként (ágak: `if (<cond>)`, `elseif (<cond>)`, `else`; az `if let` feltétele
`let Some(x) = y`), és `match` (`cond` = a vizsgált kifejezés, ágak: a minta, őrfeltételnél `<minta> if <őr>`, a kar
törzsének sortartományával). A `cond` ≤80. Ciklus (`for`, `while`, `loop`), `let … else` és `?` nem kerül be.

## strings
`StringLit`: a törzsekben és a makró-argumentumokban lévő string-literálok (`"…"`, raw string is), az átlátszatlan
makrók tokenjeiben lévők, valamint a függvények és típusok (nem doc) attribútumaiban lévők (`#[get("/users/{id}")]`,
`#[serde(rename = "x")]`). Üres / csak whitespace kimarad; ≤300 karakter; többsoros literálnál `endLine`.

## comments
A syn a sima kommenteket eldobja, ezért egy kis lexer (`comment_scan.rs`) gyűjti őket (string-, raw-string-, karakter-
literál és élettartam átugrásával, beágyazott blokk-kommenttel). A doc-kommentek (`///`, `//!`, `/** */`, `/*! */`)
kimaradnak (azok a `doc` mezőkbe kerülnek). Felvétel: `TODO` / `FIXME` / `XXX` / `HACK` szó (`tag`) vagy ≥15 karakter;
`text` ≤150. `caller` = a legszűkebb tartalmazó függvény, különben `__top__` (a container a legszűkebb tartalmazó).

## Ismert korlátok
- **Makrók nincsenek kibontva**: a derive-, attribútum- és függvényszerű makrók által generált kód nem látszik; a makró
  törzse csak akkor elemzett, ha kifejezés-lista vagy utasítás-sorozat.
- **Nincs típus-következtetés**: `x.bar()` ismeretlen fogadónál `external_call`; a gyakori std-metódusnevek
  (`len`, `push`, `get`…) `native_rust`-ok akkor is, ha egy saját típus metódusai — kivéve, ha a név pontosan egy
  saját containerben létezik `self`-es metódusként, mert az elsőbbséget kap.
- **Glob-import** (`use crate::x::*`, `use super::*`) tartalma nem ismert: a nagybetűs / egyszegmensű ismeretlen név
  `module_use` (`resolved` nélkül), kivéve a prelude- és gyakori std-neveket.
- A `cfg`-attribútumok nem értékelődnek ki: minden ág (pl. `#[cfg(test)] mod tests`) bekerül.
- Szintaktikai hibás fájlból csak a fájl-container és a figyelmeztetés készül (a syn nem hibatűrő).
- Az inline modulok importjai egy, fájl-szintű névtáblába kerülnek (azonos aliasnál az első nyer).
- A bejáró nem hagyja ki a `target/` mappát (a generált `.rs` fájlok is bejárásra kerülhetnek); érdemes a projektet a
  forrásmappára (`src`) indítani.
