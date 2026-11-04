# callgraph — architektúra és kódolási szabályok

A PHP-elemző Rust-portja. Kimenete **JSONL**: elemzett fájlonként egy sor, egy JSON-objektum — a tartalom
ugyanaz, mint a `callgraph` skill `ast_graph.php`-jáé (mérce: `golden/` —
`python3 golden/compare.py --ref golden/ref.jsonl --cmd "target/release/callgraph {file}"`).

## Alapelvek (felhasználói követelmény, 2026-11-04)

1. **Külön fájlokból épül fel.** Egy fájl = egy felelősség (egy fő típus vagy egy szorosan összetartozó kis
   csoport). Nincs „god-file”: ~300 sor fölött a fájlt szét kell bontani. A `main.rs` csak a CLI-belépő.
2. **Objektumorientált (Rust-módra).** Állapot + viselkedés `struct`-ban, `impl`-blokkokkal; a kiterjesztési
   pontok `trait`-ek; a függőségek a konstruktoron át jönnek (dependency injection, generikus vagy `Box<dyn Trait>`).
   Öröklés helyett kompozíció. Publikus felület minimális (`pub(crate)` alapból).
3. **Tervezési minták** — ahol a probléma kéri, nem öncélúan:

| Minta | Hol | Mit old meg |
|---|---|---|
| **Facade** | `analysis::Analyzer` | egy belépő: fájl → `FileGraph`; a CLI csak ezt látja |
| **Adapter** | `parser::PhpParser` trait + `parser::MagoAdapter` | a külső parser-crate (mago-syntax) elrejtése; cserélhető |
| **Visitor** | `analysis::visitor::*` | az AST bejárása szempontonként (struktúra, hívások, vezérlés, stringek, kommentek, írások) |
| **Builder** | `model::FileGraphBuilder` | a visitorok ebbe gyűjtenek; a végén immutábilis `FileGraph` |
| **Chain of Responsibility** | `categorize::CategoryChain` + `categorize::rules::*` | hívás-kategória: szabályok sorban, az első illeszkedő dönt (model_use, internal_helper, …) |
| **Strategy** | `framework::FrameworkStrategy` (`laravel`, `yii`, `generic`) | framework-specifikus ismert nevek / kategóriák |
| **Factory** | `framework::FrameworkDetector` | composer.json alapján a megfelelő stratégia (könyvtáranként cache-elve) |
| **Repository** | `io::cache::CacheStore` trait + `JsonlCacheStore` | mtime-alapú cache JSONL-ben, cserélhető tároló |
| **Command** | `cli::commands::*` | CLI-alparancsok (elemzés, cache-takarítás, …) egységes felülettel |

## Modul- és fájlszerkezet (cél)

```
src/
  main.rs                 CLI-belépő (argumentumok → Command → futtatás)
  lib.rs                  modulok exportja
  error.rs                közös hiba-típus (thiserror)
  cli/      mod.rs args.rs command.rs commands/{analyze.rs,...}
  model/    mod.rs file_graph.rs builder.rs container.rs method.rs property.rs constant.rs
            call.rs control_flow.rs string_lit.rs comment.rs write.rs      (serde, a JSON-séma 1:1)
  parser/   mod.rs (trait PhpParser, ParsedFile) mago_adapter.rs
  analysis/ mod.rs analyzer.rs name_resolver.rs type_tracker.rs context.rs
            visitor/{mod.rs, structure.rs, calls.rs, control_flow.rs, strings.rs, comments.rs, writes.rs}
  categorize/ mod.rs chain.rs rule.rs rules/{model_use.rs, internal_helper.rs, i18n.rs, native_php.rs, ...}
  framework/  mod.rs strategy.rs detector.rs laravel.rs yii.rs generic.rs
  io/       mod.rs walker.rs cache.rs jsonl_writer.rs
tests/      golden.rs (integrációs: a golden/ korpuszon), és modulonkénti unit tesztek a fájlokban (#[cfg(test)])
```

## Parser (R2 döntés, 2026-11-04)

`mago-syntax = "=1.59.0"` (pontos verzió — az API verziók között változhat). Indoklás és API-jegyzet:
`spike/DONTES.md`. A 4 mintafájlon 72/72 konténer/metódus-sor egyezik a nikic-referenciával; parse-idő
~0,74 ms a 2771 soron (tree-sitter-php: ~9,1 ms). Névfeloldáshoz a `mago-names = "=1.59.0"` a jelölt (R6).
Figyelni: a stringek `&[u8]`-ként jönnek; a docblock nincs a node-hoz kötve, a trivia-listából kell párosítani.
Minden mago-specifikus típus csak a `parser/` és az `analysis/visitor/` alatt jelenhet meg.

## Szabályok a kódhoz

- `cargo fmt` + `cargo clippy -- -D warnings` tisztán; edition 2024.
- Minden publikus típusnak/traitnek rövid doc-kommentje van (miért létezik, nem regény).
- **Formátum: JSONL mindenhol, JSON-fájl nem.** A kimenet, a cache és a golden referencia is soronként egy
  JSON-objektum (egy fájl elemzése = egy sor; nincs tördelt / pretty JSON, nincs fájlonkénti `.json`).
  A cache egy JSONL tároló (kulcs: abszolút út; mtime + az eszköz verziója), írás hozzáfűzéssel, takarításkor
  tömörítés (a legutóbbi érvényes sor marad). Konfigurációt (`composer.json`, `jsconfig.json`) természetesen JSON-ként olvasunk.
- A mezőnevek és -sorrend a `golden/ref.jsonl` szerint (serde `rename`, `skip_serializing_if` ahol a referencia
  is elhagyja).
- Egy lépés = egy szűk, golden-mérhető változás (l. a terminál-todo R3–R12 lépéseit).
- Release profil: `lto = true`, `codegen-units = 1`, `strip = true`, `opt-level = 3`.
- Git-parancsot az agentek nem futtatnak (a felhasználó dönt a verziókezelésről).
