# Új nyelv hozzáadása

Egy nyelv egy `src/lang/<nyelv>/` modul, ami a `Language` traitet (és egy szerep-traitet) implementálja, plus egy
regisztrációs sor. Az `Analyzer`, a bejáró, a cache és a CLI nem változik. A működő minta: `tests/languages.rs`
(három teszt-nyelv), a teljes példa: `src/lang/php/`.

## 0. Közös lépések (minden szerepnél)

1. **Szerep választása**: backend (szerveroldali kód), frontend (kliensoldali kód / stílus), document (adat / szöveg).
2. **Modul**: `src/lang/<nyelv>/mod.rs` + `backend.rs` / `frontend.rs` / `document.rs` (a struct és az impl-ek),
   kiegészítő fájlok: `parser.rs` (vagy `parser/`, ha külső crate-et kell elrejteni — Adapter), visitorok, szabályok.
   Egy fájl ≤ ~300 sor, egy felelősség.
3. **`Language` implementálása**:
   ```rust
   impl Language for MyLang {
       fn id(&self) -> &'static str { "python" }                    // stabil azonosító (a cache-kulcsban is szerepel)
       fn role(&self) -> LanguageRole { LanguageRole::Backend }
       fn extensions(&self) -> &'static [&'static str] { &["py"] }   // pont nélkül, kisbetűvel
       fn analyze(&self, file: &SourceFile) -> Result<Analysis> { /* … */ }
   }
   ```
   Az `analyze` a `FileGraphBuilder`-be gyűjt (`FileGraphBuilder::new(file.path_string())`, majd `container`, `method`,
   `constant`, `import`, …), a végén `Analysis { graph: builder.build(), warnings }`. Nem végzetes hibák (pl. szintaktikai
   hiba) a `warnings`-ba kerülnek; csak végzetes hiba lehet `Err`.
4. **Függőségek befecskendezése**: a parser, visitorok a konstruktoron át jönnek (`new(...)`), plusz egy `with_defaults()`
   az alapértelmezett összeállításnak — mint a `PhpBackend`-nél.
5. **Modul bekötése**: `src/lang/mod.rs` → `pub mod <nyelv>;`.
6. **Regisztráció**: `LanguageRegistry::with_defaults()` (`src/lang/registry.rs`) — egy sor a szerepnek megfelelő
   `register_*` hívással. Innentől a bejáró a kiterjesztést is bejárja, és az `Analyzer` az új nyelvre küld.
7. **Golden / mérce**: fájl-szintű referencia a nyelv saját mezőire (mérce: a nyelv hivatalos parsere vagy kézzel
   ellenőrzött elvárt kimenet). Ha nincs projekt-minta, mintakészlet a `golden/samples/<nyelv>/` alatt. Mérés:
   `python3 golden/compare.py --ref <ref.jsonl> --cmd "target/release/callgraph {file}" --fields <mező> --show 5`.
8. **Tesztek**: unit tesztek a fájlokban (`#[cfg(test)]`), plusz a registry-választás: a kiterjesztés → nyelv, szerep,
   és hogy a bejáró a kiterjesztést járja (`tests/languages.rs` mintájára).
9. **Elfogadás**: `cargo fmt`, `cargo clippy --release -- -D warnings`, `cargo test --release`, a PHP kimenet bájtra
   azonos (`bash tools/identity_check.sh target/release/callgraph` → AZONOS), a golden egy régi mezőben sem romlik.

## 1. Backend-nyelv (pl. Python, Go)

- **Fájlok**: `src/lang/python/{mod.rs, backend.rs, parser.rs, visitor/…, framework.rs}`.
- **Trait**: `LanguageBackend: Language` — `fn detect_framework(&self, dir: &Path) -> Option<String>`
  (pl. `pyproject.toml` / `manage.py` → `"django"`; `go.mod` → modul neve). A PHP-nál ez a `FrameworkDetector` (Factory)
  + `FrameworkStrategy` (Strategy); a hívás-kategorizálás a `core::Chain`-re építhető (Chain of Responsibility).
- **Leképezés**: osztály/függvény → container/method, hívások → `calls`, importok → `imports`, framework → `framework`.
- **Regisztráció**: `registry.register_backend(Box::new(PythonBackend::with_defaults()));`
- **Mérce**: a nyelv szabványos AST-modulja (pl. Python `ast`) vagy a `callgraph` skill megfelelője.

## 2. Frontend-nyelv (pl. CSS, SCSS, JS, TS, Vue)

- **Fájlok**: `src/lang/css/{mod.rs, frontend.rs, parser.rs, …}`.
- **Trait**: `LanguageFrontend: Language` — `fn resolve_module(&self, spec: &str, from: &Path) -> Option<PathBuf>`
  (JS/TS: `import` specifikátor, `jsconfig.json` / `tsconfig.json` alias; SCSS: `@use` / `@import` partial-feloldás;
  relatív útból a `from` melletti fájl, ha létezik).
- **Leképezés** (CSS/SCSS): container = a stíluslap; „metódus” = mixin / function / szabály-blokk; „hívás” = `@include`,
  `@extend`, `var(--x)`; imports = `@use` / `@forward` / `@import`; constants = `$változó`, `--custom-property`;
  controlFlow = `@if` / `@each` / `@for` / `@media`.
- **Regisztráció**: `registry.register_frontend(Box::new(CssFrontend::with_defaults()));`
- **Mérce**: a projekt saját fájljai + kézzel ellenőrzött elvárt kimenet (SCSS-hez saját mintakészlet).

## 3. Document-nyelv (pl. JSON, Markdown, YAML, TOML)

- **Fájlok**: `src/lang/json/{mod.rs, document.rs, parser.rs, …}`.
- **Trait**: `LanguageDocument: Language` — `fn resolve_reference(&self, reference: &str, from: &Path) -> Option<PathBuf>`
  (JSON: `$ref`, `extends`; Markdown: relatív link; a külső URL-re `None`).
- **Leképezés**: JSON — container: a dokumentum; constants: kulcs-útvonalak típussal és sorral; imports: hivatkozások.
  Markdown — container: a dokumentum; „metódus”: címsor-szakaszok sortartománnyal; „hívás”: linkek; strings: kódblokkok nyelve.
- **Regisztráció**: `registry.register_document(Box::new(JsonDocument::with_defaults()));`
- **Mérce**: a projekt JSON- / MD-fájljai; a `json-outline` / `markdown-outline` szerkezete támpont.

## Ellenőrzőlista

- [ ] `Language` + szerep-trait implementálva, `extensions()` pont nélküli, kisbetűs
- [ ] `src/lang/mod.rs`: `pub mod <nyelv>;`
- [ ] `with_defaults()`: a megfelelő `register_*` sor
- [ ] unit tesztek + registry-teszt (kiterjesztés, szerep, bejárás, ismeretlen kiterjesztés kimarad)
- [ ] golden / mintakészlet a nyelv mezőire
- [ ] a PHP kimenet bájtra azonos, fmt + clippy + test tiszta
- [ ] fájlok ≤ ~300 sor, minden publikus típushoz doc-komment
