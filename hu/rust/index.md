> Rust-crate-ek térképe AI agenteknek: structok, traitek, impl-blokkok, use-fák, makróhívások, framework a Cargo.toml-ból, és hogy ki mit hív.

Source: https://callgraph.eu/hu/rust/

# Rust call graph és hatáselemzés AI agenteknek

A callgraph a [syn](https://github.com/dtolnay/syn)-nel elemzi a `.rs` fájlokat, és megadja az agentnek a modulokat, structokat, traiteket és `impl`-blokkokat pontos sortartománnyal, minden hívást kategóriánként, és hogy ki hív egy függvényt a crate-ben — a fájl beolvasása nélkül.

[Telepítés](https://callgraph.eu/hu/#install) [Használat MCP-n át](https://callgraph.eu/hu/mcp/)

## Mit nyer ki a callgraph a Rust-kódból

- A fájl-modul, az inline `mod`-ok, a `struct`-ok, `enum`-ok, `union`-ok és `trait`-ek (a supertraitekkel).

- Az `impl`-blokkok metódusai a típusukhoz kerülnek; a trait-implementációk az `implements` alatt látszanak.

- Függvények láthatósággal (`pub`, `pub(crate)`…), `async`-kal, paraméterekkel és visszatérési típussal; `macro_rules!` makrók.

- Mezők, `const` / `static` elemek, típus-aliasok és enum-variánsok.

- Kilapított `use`-fák, út- / metódus- / makróhívások, `if` / `match` ágak, string-literálok (az attribútumokban is, pl. `#[get("/users/{id}")]`).

## Framework-ismeret: axum, actix-web, tokio…

A framework a fájl fölötti legközelebbi `Cargo.toml`-ból jön: az első ismert függőség, ebben a sorrendben — bevy, tauri, leptos, dioxus, yew, axum, actix-web, rocket, warp, poem, diesel, sqlx, sea-orm, clap, tokio, async-std, serde. Az ismert framework-crate-ek hívásai `framework_use` címkét kapnak, a [`profile`](https://callgraph.eu/hu/profile/) pedig felismeri a Rust webes architektúrát.

| Kategória | Mikor |
|---|---|
| `internal_helper` | a fájl saját típusai, függvényei és metódusai (`self.x()`, `Self::x()`) |
| `module_use` | a crate másik modulja (`crate::`, `super::`), `modul::fn` / `Típus::fn` alakban |
| `native_rust` | std / core / alloc, a prelude és a gyakori std-metódusok |
| `framework_use` | ismert framework-crate (axum, tower, hyper, actix_web, rocket, tokio, serde, clap, diesel, sqlx, bevy…) |
| `package_use` | más crate (külső függőség) |
| `macro_use` | minden makróhívás; a makró nincs kibontva |
| `external_call` | helyi változó vagy closure, illetve ismeretlen fogadójú metódus (`?::név`) |

## Parancsok

```
CG=~/.claude/skills/callgraph/callgraph.sh
bash $CG --project myapp src/api/users.rs              # outline: struct/impl/trait, fn-ek sortartománnyal
bash $CG --project myapp src/api/users.rs --format api # pub elemek, az impl-blokkok pub fn-jei is
bash $CG --project myapp src --const MAX_ITEMS         # const / static (Típus::CONST is): definíció + használat
```

A `--const` azt is megmutatja, hol adnak tovább egy nevet: a `pub use state::{CheckState, STATE_FILE};` például `[re-export] state::STATE_FILE` sorként jelenik meg.

## Ki hívja ezt a Rust-függvényt?

```
bash $CG --project myapp src --grep 'Repo::find'                # crate-belső: Típus::fn / modul::fn
bash $CG --project myapp src --grep '(^|::|\.|->)find$'         # a név minden hívása
bash $CG --project myapp src --grep 'Repo::find' --depth 3      # a hívók hívói
```

Az értéken hívott metódusnál (`x.load()`) a callgraph az egyszerű, megbízható esetekben kikövetkezteti az `x` típusát: paraméter-típus (`x: &Repo`, `Box` / `Rc` / `Arc`, extractor, mint a `State(s): State<App>`), `let x = Repo::new()`, struct-literál, `self`, vagy a saját struct mezője (`self.repo.load()`). Ilyenkor a hívás `Repo::load` lesz. Ha a típus nem következtethető ki, `?::load` marad, és a `Repo::load` keresés bizonytalanként ezt is listázza — nem esik ki csendben.

## Korlátok

- A makrók nincsenek kibontva: a derive-, attribútum- és függvényszerű makrók által generált kód nem látszik.

- A típus-következtetés szándékosan egyszerű: a visszatérési típusok és a láncolt hívások (`a.b().c()`) nem kapnak típust.

- A `cfg`-attribútumok nem értékelődnek ki: minden ág bekerül, pl. a `#[cfg(test)] mod tests` is.

- Szintaktikai hibás fájlból csak a fájl-container és egy figyelmeztetés készül.

- A bejáró nem hagyja ki a `target/` mappát; a callgraph-ot a `src` mappára indítsd.

## Kapcsolódó oldalak

[

### locate

Rangsorolt fájlok a feladat kezdéséhez.

](https://callgraph.eu/hu/locate/) [

### Projekt-profil

Egy crate architektúrája és rétegei.

](https://callgraph.eu/hu/profile/) [

### MCP-szerver

Ugyanezek a lekérdezések bármely MCP-kliensben.

](https://callgraph.eu/hu/mcp/) [

### Swift / SwiftUI

Egy másik natív nyelv, amit a callgraph feltérképez.

](https://callgraph.eu/hu/swift/)
