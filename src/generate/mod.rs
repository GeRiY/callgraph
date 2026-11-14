//! Kódgenerátor: egy rövid, Emmet-szerű kifejezésből projekt-tudatos, szintaktikailag ellenőrzött PHP-kód
//! (`callgraph gen`, MCP `generate`). A kimeneti token a legdrágább, ezért a generátor maga írja a fájlt, és csak
//! egy sort ad vissza (kódot csak `--dry`-nál).
//!
//! A modul neve `generate` (a `gen` az edition 2024-ben fenntartott kulcsszó). A szerkezet tervezési mintái:
//! - **Interpreter** — `lexer` (tokenizáló) → `parser` / `head` (fa, majd nyelvfüggetlen AST: `ast`);
//! - **Strategy** — az `Emitter` trait; ma `php::PhpEmitter`, később más nyelv a parser érintése nélkül;
//! - **Builder** — `php::PhpFileBuilder`: névtér, rendezett importok, `strict_types`, törzs;
//! - **Chain of Responsibility** — `resolve`: rövid név → FQCN: explicit `use:` → projekt osztály-index →
//!   a projektben már használt importok → PHP globális osztályai → feloldatlan;
//! - **Command** — `command`: `WriteNew` / `InsertInto` / `ReplaceRange` / `Undo`, mind mentéssel, így az undo egységes;
//! - **Memento** — `backup`: `.callgraph/backups/<út>.<UTC yyyymmddThhmmss>.bak`, fájlonként az utolsó 10;
//! - **Facade** — `Generator`: a CLI és az MCP ezt hívja; a fájlrendszer és az óra DI-trait mögött (`fs`).
//!
//! Biztonság: létező fájlt a `--write` soha nem ír felül; minden módosítást mentés előz meg; a teljes új szöveget
//! a nyelv parserével ellenőrizzük írás előtt (hibánál semmi nem íródik).
//!
//! # Nyelvtan (rövid)
//! Operátorok: `>` gyerek, `+` testvér (az új sor is), `^` fel, `*n` ismétlés, `$` számláló a névben (`$$` kitöltve),
//! `( )` csoport, `{ … }` nyers törzs, `[ … ]` lista. Az elem-fejben nincs szóköz (zárójelen belül lehet).
//! - típus: `c:` osztály, `i:` interfész, `t:` trait, `e:` enum, `s:` struct; `!` final, `~` abstract, `%` readonly
//!   (`c!%:Név`); `<Ős>` extends, `[A,B]` implements; `e:Status:string(Draft=draft,Paid=paid)`
//! - tag: `+` public, `-` private, `#` protected, majd `~` abstract, `!` final, `%` readonly, `::` static;
//!   `név(Típus $p=3):Típus{törzs}`, `ctor(-Client $http)` (a láthatóság-előtagos paraméter promoted property),
//!   `$név:Típus=érték`, `K=érték`
//! - direktíva: `ns:App\Services`, `use:Foo\Bar,Baz\Qux as Q`
//! - típusok: `int float bool str bytes void mixed`, `list<T>`, `map<K,V>`, `opt<T>` / `?T`, `iter<T>`, `A|B`,
//!   felhasználói és natív PHP-nevek (`string`, `array`, `self` …)

pub mod ast;
pub mod backup;
pub mod command;
pub mod edit;
pub mod emitter;
pub mod error;
pub mod fs;
pub mod generator;
pub mod head;
pub mod lexer;
pub mod parser;
pub mod php;
pub mod project;
pub mod resolve;
pub mod vue;

#[cfg(test)]
mod tests;

pub use emitter::Emitter;
pub use error::{GenError, GenResult};
pub use fs::{Clock, Env, FileSystem, RealFs, SystemClock};
pub use generator::{GenRequest, Generator, Output, ReplaceAt, Target};
pub use php::PhpEmitter;
pub use vue::{VueGenerator, wants_vue};

/// A globális kapcsoló (`callgraph mcp --generate`).
pub const GENERATE_FLAG: &str = "--generate";
/// A környezeti változó (`1` / `true` / `yes` / `on`: be); a belépő a `--generate` kapcsolóra is ezt állítja be.
pub const GENERATE_ENV: &str = "CALLGRAPH_GENERATE";

/// A globális `--generate` kapcsoló kivétele az argumentumok közül (az első `--` előtt); `true`, ha szerepelt.
pub fn take_generate(args: &mut Vec<String>) -> bool {
    let end = args.iter().position(|a| a == "--").unwrap_or(args.len());
    let before = args.len();
    let mut at = 0;
    args.retain(|a| {
        at += 1;
        at > end || a != GENERATE_FLAG
    });
    args.len() != before
}

/// Be van-e kapcsolva a `generate` MCP-eszköz (alapból nincs: a szerver „csak olvas” ígérete változatlan).
pub fn enabled() -> bool {
    std::env::var(GENERATE_ENV).is_ok_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}
