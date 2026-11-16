//! Preset: egy kifejezésből egy teljes, következetes, több fájlos feature (`callgraph gen 'laravel:…'`, MCP
//! `generate`). Ma egy van: `laravel:` — FormRequest + controller-metódusok + route-ok + Inertia / Vue oldal, a
//! projekt saját stílusában (PSR-4 névtér, a kérések szabály-alakja, a Vue-projekt TS / JS, pontosvessző, behúzás és
//! komponensei). A kimenet egy-egy sor fájlonként; kódot csak a `--dry` ír ki (`== <út>` fejlécekkel).
//!
//! # Nyelvtan
//! `laravel:Fee[auth,prefix=admin]>create(name:str,amount:int,paid?:bool,note?:text)+edit+index`
//! - erőforrás: StudlyCase modellnév (`Fee`); kapcsolók: `[auth]` (a route-ok a meglévő `Route::middleware('auth')
//!   ->group(…)` csoportba kerülnek a `routes/web.php`-ban; ha nincs, új csoport lesz a fájl végén),
//!   `[prefix=admin]` (URI `/admin/fees`, route-nevek `admin.fees.*`);
//! - akciók `+`-szal: `index`, `create` (= create oldal + store), `edit` (= edit oldal + update), `show`, `destroy`;
//!   a `create` / `edit` mezőket vár (az egyik a másikét örökli, ha üres);
//! - mező: `név:típus`, `?` = nem kötelező (nullable): `paid?:bool` / `note:?text`; típusok: `str int float bool text
//!   date datetime email` (alapérték `str`).
//!
//! # Mit ír
//! - `app/Http/Requests/Store<M>Request.php` (`create`) / `Update<M>Request.php` (`edit`): `authorize()` → `true`,
//!   `rules()` a típusból (`str` → `required|string|max:255`, `text` → `required|string`, `int` → `required|integer`,
//!   `float` → `required|numeric`, `bool` → `boolean` (sosem kötelező), `date` / `datetime` → `required|date`,
//!   `email` → `required|email`; az opcionális `nullable` a `required` helyett);
//! - `app/Http/Controllers/<M>Controller.php`: új fájl, vagy a metódusok beszúrása a meglévő osztály végére (létező
//!   metódusra hiba); az átirányítás `to_route('<m>.index')`, ha van index route, különben `back()`;
//! - `routes/web.php`: a route-sorok (a meglévő nevű vagy URI-jú route-ot kihagyja és jelzi) + a controller importja;
//! - `resources/js/Pages/<Ms>/{Create,Edit,Index,Show}.vue`: `useForm` pontosan a kérés mezőivel, `InputLabel` /
//!   `TextInput` / `InputError` / `PrimaryButton`, ahol a projektben van (különben sima HTML), `AuthenticatedLayout`.
//!
//! # Tranzakció és undo
//! Előbb minden fájl teljes új szövege elkészül és ellenőrződik (PHP: mago, Vue: oxc / tag-egyensúly), és csak ezután
//! íródik ki mind — egyetlen hiba esetén semmi. A meglévő fájlok előtt mentés készül, és egy napló (`.callgraph/
//! backups/.last-group`) rögzíti a csoportot: a következő fájl nélküli `callgraph undo` az összes fájlt egy lépésben
//! visszaállítja (az új fájlokat törli); egy későbbi egyfájlos szerkesztés után az `undo` újra az egy fájlé.
//!
//! # Ellenőrzés
//! A kimenet végén: `✓ form ↔ rules (N fields)` (a generált oldalak `useForm` mezői ↔ a kérések szabályai, a
//! Laravel-modul űrlap-szerződés logikájával) és a route-nevek feloldása; a meglévő modell (`app/Models/<M>.php`) mezőire
//! figyelmeztetés (nem hiba): nem oszlop a migrációkban, nincs a `$fillable`-ben; hiányzó modellt jelzi (migrációt nem
//! generál).

mod check;
mod page;
mod php;
mod plan;
mod spec;

#[cfg(test)]
mod tests;

use super::PhpEmitter;
use super::command::{GroupFile, WriteGroup};
use super::emitter::Emitter;
use super::error::{GenError, GenResult};
use super::fs::Env;
use super::generator::{GenRequest, Output, Target};
use super::vue;
use plan::Planned;

/// A preset-nek szól-e a kérés: a kifejezés feje `laravel:`.
pub fn wants_preset(request: &GenRequest) -> bool {
    request.expr.trim_start().starts_with(spec::HEAD)
}

/// A Facade.
pub struct PresetGenerator<'a> {
    env: Env<'a>,
}

impl<'a> PresetGenerator<'a> {
    pub fn new(env: Env<'a>) -> Self {
        Self { env }
    }

    /// A kérés végrehajtása: terv → ellenőrzés (minden fájl) → egy tranzakcióban írás (vagy `--dry`: kiírás).
    pub fn run(&self, request: &GenRequest) -> GenResult<Output> {
        if request.target != Target::Preview {
            return Err(GenError::new(
                crate::t!(
                    "a preset picks its files itself: use `gen 'laravel:…'` without --write / --into / --replace (--dry previews)",
                    "a preset maga választja a fájlokat: `gen 'laravel:…'` kapcsoló nélkül (--write / --into / --replace nem kell; a --dry csak előnézet)"
                )
                .to_owned(),
            ));
        }
        let spec = spec::parse(&request.expr)?;
        let names = spec::Names::of(&spec);
        let plan = plan::build(self.env, &spec)?;
        for file in &plan.files {
            validate(file)?;
        }
        let mut tail = plan.notes.clone();
        tail.extend(check::self_check(&plan));
        tail.extend(check::model_notes(
            self.env,
            &plan,
            &names,
            &spec.form_fields(),
        ));

        let mut lines: Vec<String> = Vec::new();
        if request.dry {
            for file in &plan.files {
                lines.push(format!("== {}", file.rel));
                lines.extend(file.shown.iter().cloned());
            }
        } else {
            let group = WriteGroup {
                files: plan
                    .files
                    .iter()
                    .map(|f| GroupFile {
                        rel: f.rel.clone(),
                        text: f.text.clone(),
                    })
                    .collect(),
            };
            group.execute_all(self.env)?;
            lines.extend(plan.files.iter().map(|f| f.summary.clone()));
        }
        lines.extend(tail);
        if !request.dry {
            lines.push(
                crate::t!(
                    "undo: `callgraph undo` reverts all of these in one step",
                    "visszavonás: a `callgraph undo` egy lépésben mindet visszaállítja"
                )
                .to_owned(),
            );
        }
        Ok(Output {
            lines,
            unresolved: Vec::new(),
        })
    }
}

/// Egy tervezett fájl szintaxis-ellenőrzése írás előtt (PHP: mago, Vue: oxc / tag-egyensúly / CSS).
fn validate(file: &Planned) -> GenResult<()> {
    if file.rel.ends_with(".php") {
        PhpEmitter
            .check(&file.rel, &file.text)
            .map_err(|(line, message)| {
                GenError::new(crate::tf!(
                    "syntax error in {} at L{}: {} (nothing written)",
                    "szintaktikai hiba itt: {} az L{}. sorban: {} (nem írtam semmit)",
                    file.rel,
                    line,
                    message
                ))
            })
    } else if file.rel.ends_with(".vue") {
        vue::check_sfc(&file.text).map_err(|message| {
            GenError::new(crate::tf!(
                "syntax error in {}: {} (nothing written)",
                "szintaktikai hiba itt: {}: {} (nem írtam semmit)",
                file.rel,
                message
            ))
        })
    } else {
        Ok(())
    }
}
