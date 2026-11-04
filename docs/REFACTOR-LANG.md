# Refactor: többnyelvű réteg — LanguageBackend + LanguageFrontend (R2)

Felhasználói követelmény (2026-11-04): a rendszer ne PHP-hez kötött legyen, hanem **nyelv-modulokból** álljon.
Két szerep: **LanguageBackend** (szerveroldali nyelvek — most PHP, később pl. Python, Go) és **LanguageFrontend**
(kliensoldali nyelvek — később JS / TS / Vue). Egy új nyelv = egy új `src/lang/<nyelv>/` modul; a közös rétegekhez nem kell nyúlni.

## Nyelvek és szerepek (2026-11-04, bővítve)

| Szerep | Trait | Nyelvek | Mérce (golden) |
|---|---|---|---|
| szerveroldal | `LanguageBackend` | PHP (kész), később Python, Go… | `ast_graph.php` |
| kliensoldal | `LanguageFrontend` | CSS, SCSS (R3), később JS / TS / Vue | CSS: a projekt 56 fájlja + kézzel ellenőrzött elvárt kimenet; **SCSS: a projektben nincs — saját mintakészlet** (`golden/samples/scss/`) |
| dokumentum / adat | `LanguageDocument` (R2f) | JSON, Markdown (R4), később YAML, TOML | a projekt JSON- és MD-fájljai; a `json-outline` / `markdown-outline` skill szerkezete támpont |

A közös `FileGraph` sémába a nem-kód nyelvek is leképezhetők (a leképezést az R3a / R4a rögzíti):
CSS/SCSS — container: a stíluslap; „metódus”: mixin / function / szabály-blokk; „hívás”: `@include`, `@extend`, `var(--x)`, függvény;
imports: `@use` / `@forward` / `@import`; constants: `$változó`, `--custom-property`; controlFlow: `@if` / `@each` / `@for` / `@media`.
JSON — container: a dokumentum; constants: kulcs-útvonalak típussal és sorral; imports: hivatkozások (`$ref`, `extends`).
Markdown — container: a dokumentum; „metódus”: címsor-szakaszok sortartománnyal; „hívás”: linkek (belső / külső); strings: kódblokkok nyelve.

## Célszerkezet

```
src/
  main.rs, lib.rs, error.rs
  core/                         nyelvfüggetlen építőkövek
    chain.rs                    Chain<I> + Rule<I> — generikus Chain of Responsibility (a PHP kategória-lánc erre épül)
    source.rs                   SourceFile { path, text, mtime } — a nyelvek bemenete
  lang/                         a nyelv-réteg
    mod.rs                      re-exportok
    language.rs                 trait Language                      (közös: id, extensions, analyze)
    backend.rs                  trait LanguageBackend: Language     (szerveroldal: framework-felismerés)
    frontend.rs                 trait LanguageFrontend: Language    (kliensoldal: modul-/alias-feloldás)
    registry.rs                 LanguageRegistry                    (Factory: kiterjesztés → nyelv; a bejáró kiterjesztés-listája)
    php/                        PhpBackend: impl LanguageBackend — MINDEN mostani PHP-specifikus kód ide költözik:
      mod.rs backend.rs         (PhpBackend: a mostani Analyzer-csővezeték: MagoAdapter + visitorok + Builder)
      parser/                   (a mostani src/parser/)
      analysis/                 (a mostani src/analysis/: visitorok, name_resolver, type_tracker, docblock, context)
      printer/                  (R1: a két pretty-printer egyesítve)
      categorize/               (a mostani src/categorize/: CallInfo, a 12 szabály, natív lista — a core::chain-re építve)
      framework/                (a mostani src/framework/: stratégiák + detektor)
  model/                        közös kimeneti séma (FileGraph + Builder) — változatlan
  analysis/                     Analyzer (Facade): fájl → registry.for_path → nyelv.analyze → FileGraph
  io/                           walker (a registry kiterjesztéseivel), cache (CacheStore + JsonlCacheStore), jsonl_writer
  cli/                          Command minta; AnalyzeCommand a registryt és egy dyn CacheStore-t kap (DI)
```

## Traitek (vázlat)

```rust
pub enum LanguageRole { Backend, Frontend, Document }

/// Egy nyelv támogatása: felismeri a fájljait és elemzi őket a közös FileGraph sémába.
pub trait Language: Send + Sync {
    fn id(&self) -> &'static str;                         // "php", később "python", "js"…
    fn role(&self) -> LanguageRole;
    fn extensions(&self) -> &'static [&'static str];      // ["php"]
    fn analyze(&self, file: &SourceFile) -> Result<FileGraph>;
}

/// Szerveroldali nyelv: framework-felismerés (composer.json, pyproject…), később útvonal-/ORM-specifikus térképek.
pub trait LanguageBackend: Language {
    fn detect_framework(&self, dir: &Path) -> Option<String>;
}

/// Kliensoldali nyelv: modul-specifikátor és alias (jsconfig/tsconfig) feloldása, később komponens-/template-elemzés.
pub trait LanguageFrontend: Language {
    fn resolve_module(&self, spec: &str, from: &Path) -> Option<PathBuf>;
}

/// Dokumentum- / adatformátum (JSON, Markdown, YAML…): nincs futó kód, de szerkezet, kulcsok, hivatkozások vannak.
pub trait LanguageDocument: Language {
    fn resolve_reference(&self, reference: &str, from: &Path) -> Option<PathBuf>;   // pl. $ref, relatív link
}

/// Factory: a regisztrált nyelvek közül kiterjesztés szerint választ.
pub struct LanguageRegistry { backends: Vec<Box<dyn LanguageBackend>>, frontends: Vec<Box<dyn LanguageFrontend>> }
impl LanguageRegistry {
    pub fn with_defaults() -> Self;                       // most: PhpBackend; frontend még nincs
    pub fn register_backend(&mut self, l: Box<dyn LanguageBackend>);
    pub fn register_frontend(&mut self, l: Box<dyn LanguageFrontend>);
    pub fn for_path(&self, p: &Path) -> Option<&dyn Language>;
    pub fn extensions(&self) -> Vec<&'static str>;
}
```

Minták: **Abstract Factory** (egy nyelv-modul a saját parserét, visitorait, szabályait, stratégiáit adja), **Factory** (registry),
**Facade** (Analyzer), a többi változatlan (Adapter, Visitor, Builder, Chain, Strategy, Repository, Command).

## Lépések (terminál-todo: R2…)

| Lépés | Mit | Ki | Párhuzamos? |
|---|---|---|---|
| R2a | A nyelv-réteg (core/, lang/ traitek + registry) és a PHP-kód áthelyezése `lang/php/` alá; Analyzer, walker, CLI a registryn át | claude2 (sonnet) — egy kézben, mert minden fájl mozog | nem (a C5 után, egyedül) |
| R2b | Printer-összevonás `lang/php/printer/` alá + a `calls.rs` bontása (R1 része) | codex (gpt-reserve, medium) | igen (R2a után) |
| R2c | Nagy fájlok bontása: `io/cache.rs`, `lang/php/categorize/rules/mod.rs` (tesztek külön) (R1 része) | codex (gpt-reserve, medium) | igen |
| R2d | Repository-DI: `AnalyzeCommand` `dyn CacheStore`-t kap; a cache-kulcsba a nyelv-id is bekerül (R1 része) | codex (gpt-reserve, medium) | igen |
| R2e | Dokumentáció + bizonyíték: ARCHITECTURE.md frissítése, „Új nyelv hozzáadása” útmutató, `tests/` alatt egy teszt-backend és teszt-frontend, ami bizonyítja a registry kiterjesztés szerinti választását | claude2 (sonnet) | igen |

## Elfogadás (minden lépésnél)

- **Bájtra azonos kimenet** a refactor előtti binárissal: `callgraph --no-cache` a két teljes referencia-korpuszon (`tools/local.env`: `CALLGRAPH_CORPUS_A`, `CALLGRAPH_CORPUS_B`) (`cmp`).
- A golden (kis + teljes korpusz) egyetlen mezőben sem romlik.
- `cargo fmt`, `cargo clippy --release -- -D warnings`, `cargo test --release` tiszta; minden fájl ≤ ~300 sor (az adatfájl kivétel).
