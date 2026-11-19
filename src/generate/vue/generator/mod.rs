//! A Vue-generátor Facade-ja: kifejezés → `Program` → projekt-ismeret (TS / JS, pontosvessző, komponens-importok)
//! → emitterek (script / template / style) → fájl (Builder) → ellenőrzés (oxc / tag-egyensúly / CSS) → parancs
//! (`WriteNew` / `InsertInto` / `ReplaceRange`, mentéssel). Semmi nem íródik, amíg a teljes új szöveg hibátlan.
//!
//! Felosztás: ez a modul a belépő (`run` elosztja a kérést) és a közös segédek; `render` a kifejezés kirajzolása
//! (szakaszok, importok) és a teljes új szövegek; `write` az előnézet és a `--write`; `insert` az `--into`;
//! `replace` a `--replace`; `sfc` a `.vue` szakaszok szerkesztése; `script_edit` a `.ts` / `.js` fájlok szerkesztése.

mod insert;
mod react;
mod render;
mod replace;
mod script_edit;
mod sfc;
mod write;

use super::project::{CodeStyle, VueProject, script_is_ts, uses_semicolons};
use crate::generate::error::{GenError, GenResult, Kind as ErrorKind};
use crate::generate::fs::Env;
use crate::generate::generator::{GenRequest, Output, ReplaceAt, Target};
use crate::generate::target::{self, Checker};
pub(super) use crate::generate::target::{Loaded, check_expect};
pub(super) use sfc::parse_line_ref;

/// A cél-fájl fajtája.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileKind {
    Vue,
    Script {
        ts: bool,
    },
    /// `.tsx` / `.jsx`: a React component.
    React {
        ts: bool,
    },
}

fn file_kind(rel: &str) -> GenResult<FileKind> {
    match rel.rsplit('.').next().unwrap_or("") {
        "vue" => Ok(FileKind::Vue),
        "tsx" => Ok(FileKind::React { ts: true }),
        "jsx" => Ok(FileKind::React { ts: false }),
        "ts" | "mts" | "cts" => Ok(FileKind::Script { ts: true }),
        "js" | "mjs" | "cjs" => Ok(FileKind::Script { ts: false }),
        _ => Err(GenError::new(crate::tf!(
            "{} should end in .vue, .ts, .tsx, .js or .jsx",
            "{} kiterjesztése .vue, .ts, .tsx, .js vagy .jsx legyen",
            rel
        ))),
    }
}

/// A Vue-generátornak szól-e a kérés: a cél-fájl `.vue` / `.ts` / `.js` / `.html`, vagy (cél nélkül) a kifejezés feje
/// `vue:` / `comp:` / `html:` / `script>` / `template>` / `style>`.
pub fn wants_vue(request: &GenRequest) -> bool {
    if super::html_gen::wants_html(request) {
        return true;
    }
    let file = match &request.target {
        Target::Preview => {
            let e = request.expr.trim_start();
            return e.starts_with("vue:")
                || e.starts_with("react:")
                || e.starts_with("hook:")
                || e.starts_with("jsx>")
                || e.starts_with("comp:")
                || e.starts_with("mod:")
                || e.starts_with("class:")
                || e.starts_with("class~:")
                || ["script>", "template>", "style>", "style!", "style:"]
                    .iter()
                    .any(|p| e.starts_with(p));
        }
        Target::Write { file } | Target::Into { file, .. } | Target::Replace { file, .. } => file,
    };
    file_kind(file).is_ok()
}

/// `--into x.vue::#id` / `::.class` / `::template#id`: a cél egy sablon-elem, ezért a kifejezés alapból sablon-markup
/// (nem kell elé a `template >`); a kifejezetten szakasz-fejű (`template`, `script`, `style`, `vue:`) marad, ahogy van.
fn template_markup_expr(request: &GenRequest) -> String {
    let Target::Into {
        file,
        class: Some(class),
        ..
    } = &request.target
    else {
        return request.expr.clone();
    };
    let selector = class.strip_prefix("template").unwrap_or(class);
    if !file.ends_with(".vue") || !selector.starts_with(['#', '.']) {
        return request.expr.clone();
    }
    let expr = request.expr.trim_start();
    let head: String = expr
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '!' | '_' | '-'))
        .collect();
    let section = matches!(
        head.as_str(),
        "template" | "script" | "vue" | "mod" | "comp"
    ) || head.starts_with("style");
    if section {
        request.expr.clone()
    } else {
        format!("template > {expr}")
    }
}

fn is_react_file(file: &str) -> bool {
    matches!(file_kind(file), Ok(FileKind::React { .. }))
}

fn is_script_file(file: &str) -> bool {
    matches!(file_kind(file), Ok(FileKind::Script { .. }))
}

pub(super) fn problem(p: &super::check::Problem) -> GenError {
    GenError::new(crate::tf!(
        "syntax error in {} at L{}: {} (nothing written)",
        "szintaktikai hiba itt: {} az L{}. sorban: {} (nem írtam semmit)",
        p.section,
        p.line,
        p.message
    ))
    .with_kind(ErrorKind::Syntax)
}

/// A Facade.
pub struct VueGenerator<'a> {
    pub(super) env: Env<'a>,
    /// A projekt-ismeret: egy kérésen belül közös (memoizált bejárások és beolvasott fájlok).
    project: VueProject<'a>,
}

impl<'a> VueGenerator<'a> {
    pub fn new(env: Env<'a>) -> Self {
        Self {
            env,
            project: VueProject::new(env.fs, env.root),
        }
    }

    /// A kérés végrehajtása.
    pub fn run(&self, request: &GenRequest) -> GenResult<Output> {
        // HTML: `html:` fejű kifejezés vagy `.html` / `.htm` cél
        if super::html_gen::wants_html(request) {
            return self.run_html(request);
        }
        // osztálytagok: `--into x.ts::Osztály` és `--replace x.ts::Osztály::tag` (a kifejezés osztálytag, nem modul-tag)
        match &request.target {
            Target::Into {
                file,
                class: Some(class),
                place,
                upsert,
                expect,
            } if is_script_file(file) => {
                return self.insert_class(
                    &request.expr,
                    file,
                    class,
                    place.after_name(),
                    *upsert,
                    expect.as_deref(),
                    request.mode,
                );
            }
            Target::Replace {
                file,
                at:
                    ReplaceAt::Method {
                        class: Some(class),
                        name,
                    },
                expect,
            } if is_script_file(file) => {
                return self.replace_member(
                    &request.expr,
                    file,
                    (class.as_str(), name.as_str()),
                    expect.as_deref(),
                    request.mode,
                );
            }
            _ => {}
        }
        let program = super::parse::parse(&template_markup_expr(request))?;
        match &request.target {
            Target::Preview => self.preview(&program),
            Target::Write { file } => self.write(&program, file, request.mode),
            Target::Into { file, place, .. } if is_react_file(file) => {
                self.insert_react(&program, file, place.after_name(), request.mode)
            }
            Target::Replace { file, .. } if is_react_file(file) => Err(GenError::new(
                crate::t!(
                    "--replace is not available for .tsx / .jsx: use --into to add, or edit the component",
                    "a --replace .tsx / .jsx fájlra nincs: hozzáadáshoz --into, különben szerkeszd a komponenst"
                )
                .to_owned(),
            )),
            Target::Into {
                file, class, place, ..
            } => self.insert_vue_or_script(
                &program,
                file,
                class.as_deref(),
                place.after_name(),
                request.mode,
            ),
            Target::Replace { file, at, expect } => {
                self.replace(&program, file, at, expect.as_deref(), request.mode)
            }
        }
    }

    pub(super) fn rel(&self, file: &str) -> GenResult<String> {
        target::rel(self.env.root, file)
    }

    fn project(&self) -> &VueProject<'a> {
        &self.project
    }

    pub(super) fn validate(&self, rel: &str, text: &str) -> GenResult<()> {
        Checker::for_path(rel).check(text).map_err(|p| problem(&p))
    }

    pub(super) fn load(&self, rel: &str) -> GenResult<Loaded> {
        target::load(self.env, rel)
    }

    /// A meglévő fájl kódstílusa: a fájlból, ami hiányzik, azt a projektből.
    fn style_of(&self, kind: FileKind, text: &str) -> CodeStyle {
        let project = match kind {
            FileKind::React { .. } => self.project().jsx_code_style(),
            _ => self.project().code_style(),
        };
        let semi = uses_semicolons(text).unwrap_or(project.semi);
        let ts = match kind {
            FileKind::Script { ts } => ts,
            FileKind::React { ts } => ts,
            FileKind::Vue if text.contains("<script") => script_is_ts(text),
            FileKind::Vue => project.ts,
        };
        CodeStyle { ts, semi }
    }
}
