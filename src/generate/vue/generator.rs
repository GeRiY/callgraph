//! A Vue-generátor Facade-ja: kifejezés → `Program` → projekt-ismeret (TS / JS, pontosvessző, komponens-importok)
//! → emitterek (script / template / style) → fájl (Builder) → ellenőrzés (oxc / tag-egyensúly / CSS) → parancs
//! (`WriteNew` / `InsertInto` / `ReplaceRange`, mentéssel). Semmi nem íródik, amíg a teljes új szöveg hibátlan.

use std::path::{Component, Path};

use super::builder::{SfcBuilder, composable_file, script_body, script_open, style_open};
use super::check::{check_script_file, check_vue};
use super::edit::{
    Doc, declared_names, find_statement, indent_block, indent_of, indent_unit, last_nonblank,
    reindent,
};
use super::imports::{ImportSet, plan, scan};
use super::model::{Element, Import, Kind, Member, Program};
use super::project::{CodeStyle, VueProject, script_is_ts, uses_semicolons};
use super::script::ScriptEmitter;
use super::template::{components, opens_element};
use super::{style, template};
use crate::generate::command::{EditCommand, InsertInto, ReplaceRange, WriteNew};
use crate::generate::edit::{range_text, split_lines};
use crate::generate::error::{GenError, GenResult};
use crate::generate::fs::Env;
use crate::generate::generator::{GenRequest, Output, ReplaceAt, Target};
use crate::lang::js::{Sfc, SfcBlockKind};

/// A cél-fájl fajtája.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileKind {
    Vue,
    Script { ts: bool },
}

fn file_kind(rel: &str) -> GenResult<FileKind> {
    match rel.rsplit('.').next().unwrap_or("") {
        "vue" => Ok(FileKind::Vue),
        "ts" | "mts" | "cts" => Ok(FileKind::Script { ts: true }),
        "js" | "mjs" | "cjs" => Ok(FileKind::Script { ts: false }),
        _ => Err(GenError::new(crate::tf!(
            "{} should end in .vue, .ts or .js",
            "{} kiterjesztése .vue, .ts vagy .js legyen",
            rel
        ))),
    }
}

/// A Vue-generátornak szól-e a kérés: a cél-fájl `.vue` / `.ts` / `.js`, vagy (cél nélkül) a kifejezés feje
/// `vue:` / `comp:` / `script>` / `template>` / `style>`.
pub fn wants_vue(request: &GenRequest) -> bool {
    let file = match &request.target {
        Target::Preview => {
            let e = request.expr.trim_start();
            return e.starts_with("vue:")
                || e.starts_with("comp:")
                || ["script>", "template>", "style>", "style!", "style:"]
                    .iter()
                    .any(|p| e.starts_with(p));
        }
        Target::Write { file } | Target::Into { file, .. } | Target::Replace { file, .. } => file,
    };
    file_kind(file).is_ok()
}

/// Egy beolvasott fájl.
struct Loaded {
    text: String,
    lines: Vec<String>,
    eol: &'static str,
}

/// A kirajzolt szakaszok és importok.
struct Rendered {
    script: Vec<String>,
    template: Vec<String>,
    style: Vec<String>,
    imports: ImportSet,
    unresolved: Vec<String>,
}

fn problem(p: &super::check::Problem) -> GenError {
    GenError::new(crate::tf!(
        "syntax error in {} at L{}: {} (nothing written)",
        "szintaktikai hiba itt: {} az L{}. sorban: {} (nem írtam semmit)",
        p.section,
        p.line,
        p.message
    ))
}

fn one_line_block(name: &str) -> GenError {
    GenError::new(crate::tf!(
        "the <{}> block must have its tags on their own lines",
        "a <{}> blokk tagjei külön sorban legyenek",
        name
    ))
}

/// A Facade.
pub struct VueGenerator<'a> {
    env: Env<'a>,
}

impl<'a> VueGenerator<'a> {
    pub fn new(env: Env<'a>) -> Self {
        Self { env }
    }

    /// A kérés végrehajtása.
    pub fn run(&self, request: &GenRequest) -> GenResult<Output> {
        let program = super::parse::parse(&request.expr)?;
        match &request.target {
            Target::Preview => self.preview(&program),
            Target::Write { file } => self.write(&program, file, request.dry),
            Target::Into { file, after, .. } => {
                self.insert(&program, file, after.as_deref(), request.dry)
            }
            Target::Replace { file, at, expect } => {
                self.replace(&program, file, at, expect.as_deref(), request.dry)
            }
        }
    }

    fn rel(&self, file: &str) -> GenResult<String> {
        let outside = || {
            GenError::new(crate::tf!(
                "{} is outside the project",
                "{} a projekten kívül van",
                file
            ))
        };
        let path = Path::new(file);
        let relative = if path.is_absolute() {
            path.strip_prefix(self.env.root).map_err(|_| outside())?
        } else {
            path
        };
        let mut parts = Vec::new();
        for component in relative.components() {
            match component {
                Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
                Component::CurDir => {}
                _ => return Err(outside()),
            }
        }
        if parts.is_empty() {
            return Err(outside());
        }
        Ok(parts.join("/"))
    }

    fn project(&self) -> VueProject<'_> {
        VueProject::new(self.env.fs, self.env.root)
    }

    fn validate(&self, rel: &str, text: &str) -> GenResult<()> {
        let result = if rel.ends_with(".vue") {
            check_vue(text)
        } else {
            check_script_file(rel, text)
        };
        result.map_err(|p| problem(&p))
    }

    fn load(&self, rel: &str) -> GenResult<Loaded> {
        let path = self.env.root.join(rel);
        if !self.env.fs.exists(&path) {
            return Err(GenError::new(crate::tf!(
                "{} not found (a new file: --write {})",
                "{} nem található (új fájl: --write {})",
                rel,
                rel
            )));
        }
        let text = self.env.fs.read(&path).map_err(|e| {
            GenError::new(crate::tf!(
                "cannot read {}: {}",
                "nem olvasható: {}: {}",
                rel,
                e
            ))
        })?;
        let (lines, eol) = split_lines(&text);
        Ok(Loaded { text, lines, eol })
    }

    /// A meglévő fájl kódstílusa: a fájlból, ami hiányzik, azt a projektből.
    fn style_of(&self, kind: FileKind, text: &str) -> CodeStyle {
        let project = self.project().code_style();
        let semi = uses_semicolons(text).unwrap_or(project.semi);
        let ts = match kind {
            FileKind::Script { ts } => ts,
            FileKind::Vue if text.contains("<script") => script_is_ts(text),
            FileKind::Vue => project.ts,
        };
        CodeStyle { ts, semi }
    }

    /// A komponens-importok: a `bound` / explicit nevek kimaradnak; a többit a projekt-index oldja fel.
    fn component_imports(
        &self,
        elements: &[Element],
        explicit: &[Import],
        bound: &[String],
    ) -> (Vec<Import>, Vec<String>) {
        let project = self.project();
        let (mut imports, mut unresolved) = (Vec::new(), Vec::new());
        for name in components(elements) {
            let known = bound.contains(&name)
                || explicit.iter().any(|i| {
                    i.default.as_deref() == Some(name.as_str()) || i.named.contains(&name)
                });
            if known {
                continue;
            }
            if let Some(source) = project.find_component(&name) {
                imports.push(Import {
                    source,
                    default: Some(name),
                    named: Vec::new(),
                });
            } else if matches!(name.as_str(), "Link" | "Head") {
                imports.push(Import {
                    source: "@inertiajs/vue3".to_owned(),
                    default: None,
                    named: vec![name],
                });
            } else {
                unresolved.push(name);
            }
        }
        (imports, unresolved)
    }

    fn render(&self, program: &Program, style: CodeStyle, bound: &[String]) -> Rendered {
        let em = ScriptEmitter {
            ts: style.ts,
            semi: style.semi,
        };
        let mut imports = ImportSet::new();
        let mut script = Vec::new();
        let mut explicit: Vec<Import> = Vec::new();
        if let Some(s) = &program.script {
            script = em.members(&s.members);
            explicit.clone_from(&s.imports);
            imports.extend(s.imports.iter().cloned());
            imports.extend(ScriptEmitter::needed_imports(&s.members));
        }
        let mut unresolved = Vec::new();
        let mut template_lines = Vec::new();
        if let Some(elements) = &program.template {
            let (found, missing) = self.component_imports(elements, &explicit, bound);
            imports.extend(found);
            unresolved = missing;
            template_lines = template::lines(elements);
        }
        let style_lines = program
            .style
            .as_ref()
            .map(|s| style::lines(&s.rules))
            .unwrap_or_default();
        Rendered {
            script,
            template: template_lines,
            style: style_lines,
            imports,
            unresolved,
        }
    }

    /// A teljes új `.vue` fájl.
    fn sfc_file(&self, program: &Program, style: CodeStyle) -> (String, Vec<String>) {
        let rendered = self.render(program, style, &[]);
        let mut builder = SfcBuilder::new();
        if program.script.is_some() || !rendered.imports.is_empty() {
            builder = builder.script(
                style.ts,
                script_body(&rendered.imports.lines(style.semi), &rendered.script),
            );
        }
        if program.template.is_some() {
            builder = builder.template(rendered.template);
        }
        if let Some(s) = &program.style {
            builder = builder.style(s.scoped, rendered.style);
        }
        (builder.build(), rendered.unresolved)
    }

    /// A teljes új composable-fájl.
    fn composable_text(&self, program: &Program, rel: &str, style: CodeStyle) -> GenResult<String> {
        let members = composable_members(program)?;
        let name = match &program.name {
            Some(n) => n.clone(),
            None => {
                let file = rel.rsplit('/').next().unwrap_or(rel);
                let stem = file.rsplit_once('.').map_or(file, |(s, _)| s).to_owned();
                crate::generate::head::is_ident(&stem)
                    .then_some(stem)
                    .ok_or_else(|| {
                        GenError::new(
                            crate::t!(
                                "name the composable: `comp:useCart > …`",
                                "nevezd el a composable-t: `comp:useCart > …`"
                            )
                            .to_owned(),
                        )
                    })?
            }
        };
        let rendered = self.render(program, style, &[]);
        let em = ScriptEmitter {
            ts: style.ts,
            semi: style.semi,
        };
        Ok(composable_file(
            &rendered.imports.lines(style.semi),
            &em.composable(&name, members),
        ))
    }

    fn preview(&self, program: &Program) -> GenResult<Output> {
        let style = self.project().code_style();
        let (text, unresolved) = match program.kind {
            Kind::Composable => (
                self.composable_text(program, "preview.ts", style)?,
                Vec::new(),
            ),
            Kind::Sfc => self.sfc_file(program, style),
            Kind::Parts => {
                if program.template.is_none() && program.style.is_none() {
                    return self.preview_members(program, style);
                }
                self.sfc_file(program, style)
            }
        };
        let rel = if program.kind == Kind::Composable {
            if style.ts { "preview.ts" } else { "preview.js" }
        } else {
            "preview.vue"
        };
        self.validate(rel, &text)?;
        Ok(Output {
            lines: text.lines().map(str::to_owned).collect(),
            unresolved,
        })
    }

    /// Csupasz tagok előnézete: a tagok sorai (egy ideiglenes függvénybe csomagolva ellenőrizve).
    fn preview_members(&self, program: &Program, style: CodeStyle) -> GenResult<Output> {
        let rendered = self.render(program, style, &[]);
        let wrapped = format!(
            "function generated() {{\n{}\n}}\n",
            rendered.script.join("\n")
        );
        let path = if style.ts { "preview.ts" } else { "preview.js" };
        check_script_file(path, &wrapped).map_err(|mut p| {
            p.line = p.line.saturating_sub(1).max(1);
            problem(&p)
        })?;
        Ok(Output {
            lines: rendered.script,
            unresolved: rendered.unresolved,
        })
    }

    fn write(&self, program: &Program, file: &str, dry: bool) -> GenResult<Output> {
        let rel = self.rel(file)?;
        let kind = file_kind(&rel)?;
        let project = self.project().code_style();
        let (text, unresolved) = match kind {
            FileKind::Vue => {
                if program.kind == Kind::Composable {
                    return Err(GenError::new(
                        crate::t!(
                            "`comp:` writes a composable: use a .ts / .js file (a .vue file takes `vue:Name > script + template + style`)",
                            "a `comp:` composable-t ír: .ts / .js fájl kell (a .vue fájlhoz `vue:Név > script + template + style` való)"
                        )
                        .to_owned(),
                    ));
                }
                self.sfc_file(program, project)
            }
            FileKind::Script { ts } => {
                if program.kind == Kind::Sfc {
                    return Err(GenError::new(
                        crate::t!(
                            "`vue:` writes a .vue file: use `comp:useName > …` for a .ts / .js composable",
                            "a `vue:` .vue fájlt ír: .ts / .js composable-hoz `comp:useNév > …` kell"
                        )
                        .to_owned(),
                    ));
                }
                let style = CodeStyle { ts, ..project };
                (self.composable_text(program, &rel, style)?, Vec::new())
            }
        };
        self.validate(&rel, &text)?;
        if dry {
            return Ok(Output {
                lines: text.lines().map(str::to_owned).collect(),
                unresolved,
            });
        }
        let line = WriteNew { rel, text }.execute(self.env)?;
        Ok(Output {
            lines: vec![line],
            unresolved,
        })
    }

    // ---- --into -----------------------------------------------------------------------------------------------

    fn insert(
        &self,
        program: &Program,
        file: &str,
        after: Option<&str>,
        dry: bool,
    ) -> GenResult<Output> {
        let rel = self.rel(file)?;
        let kind = file_kind(&rel)?;
        let Loaded { text, lines, eol } = self.load(&rel)?;
        let style = self.style_of(kind, &text);
        let mut doc = Doc::new(lines);
        let bound = bound_names(&doc.lines);
        let rendered = self.render(program, style, &bound);
        let block = match kind {
            FileKind::Vue => insert_vue(&mut doc, program, &rendered, style, after)?,
            FileKind::Script { .. } => insert_script(&mut doc, program, &rendered, style, after)?,
        };
        let text = doc.lines.join(eol);
        self.validate(&rel, &text)?;
        if dry {
            return Ok(Output {
                lines: block,
                unresolved: rendered.unresolved,
            });
        }
        let places: Vec<String> = doc
            .placed
            .iter()
            .map(|p| format!("{} L{}-{}", p.label, p.start + 1, p.start + p.len))
            .collect();
        let summary = crate::tf!(
            "inserted into {}: {}",
            "beszúrva ide: {}: {}",
            rel,
            places.join(", ")
        );
        let line = InsertInto { rel, text, summary }.execute(self.env)?;
        Ok(Output {
            lines: vec![line],
            unresolved: rendered.unresolved,
        })
    }

    // ---- --replace --------------------------------------------------------------------------------------------

    fn replace(
        &self,
        program: &Program,
        file: &str,
        at: &ReplaceAt,
        expect: Option<&str>,
        dry: bool,
    ) -> GenResult<Output> {
        let rel = self.rel(file)?;
        let kind = file_kind(&rel)?;
        let Loaded { text, lines, eol } = self.load(&rel)?;
        let style = self.style_of(kind, &text);
        let mut doc = Doc::new(lines);
        let (from, to, label) = replace_range(&doc, kind, at, &rel)?;
        if let Some(prefix) = expect {
            let current = crate::update::checksum::sha256_hex(
                range_text(&doc.lines, from + 1, to).as_bytes(),
            );
            let wanted = prefix.trim().to_ascii_lowercase();
            if wanted.len() < 4 || !current.starts_with(&wanted) {
                return Err(GenError::new(crate::tf!(
                    "L{}-{} of {} changed (expected sha256 {}…, current {}…): read it again before replacing",
                    "L{}-{} tartalma megváltozott ebben: {} (várt sha256 {}…, jelenlegi {}…): cserélés előtt olvasd újra",
                    from + 1,
                    to,
                    rel,
                    wanted,
                    &current[..12]
                )));
            }
        }
        let bound = bound_names(&doc.lines);
        let rendered = self.render(program, style, &bound);
        let (section, block) = self.replacement_block(program, kind, &rendered)?;
        let indent = doc.lines.get(from).map_or(0, |l| indent_of(l));
        let block = indent_block(&reindent(&block, indent_unit(&doc.lines)), indent);
        doc.splice(from, to, block.clone());
        doc.mark(section, from, block.len());
        match kind {
            FileKind::Vue => {
                if !rendered.imports.is_empty() {
                    put_imports(&mut doc, &rendered.imports, style)?;
                }
            }
            FileKind::Script { .. } => {
                let end = doc.lines.len();
                for e in plan(&doc.lines, 0, end, &rendered.imports, style.semi) {
                    doc.splice(e.start, e.end, e.lines);
                }
            }
        }
        let new_text = doc.lines.join(eol);
        self.validate(&rel, &new_text)?;
        if dry {
            return Ok(Output {
                lines: block,
                unresolved: rendered.unresolved,
            });
        }
        let placed = doc
            .placed
            .iter()
            .find(|p| p.label == section)
            .map_or((from, block.len()), |p| (p.start, p.len));
        let name = label.map(|n| format!(" ({n})")).unwrap_or_default();
        let summary = crate::tf!(
            "replaced {} L{}-{} → L{}-{}{}",
            "kicserélve: {} L{}-{} → L{}-{}{}",
            rel,
            from + 1,
            to,
            placed.0 + 1,
            placed.0 + placed.1,
            name
        );
        let line = ReplaceRange {
            rel,
            text: new_text,
            summary,
        }
        .execute(self.env)?;
        Ok(Output {
            lines: vec![line],
            unresolved: rendered.unresolved,
        })
    }

    /// A csere tartalma: pontosan egy szakasz (script-tagok / template / style).
    fn replacement_block(
        &self,
        program: &Program,
        kind: FileKind,
        rendered: &Rendered,
    ) -> GenResult<(&'static str, Vec<String>)> {
        if kind != FileKind::Vue {
            composable_members(program)?;
            return Ok(("members", rendered.script.clone()));
        }
        if program.sections() != 1 {
            return Err(GenError::new(
                crate::t!(
                    "--replace takes exactly one section (`script>…`, `template>…` or `style>…`)",
                    "a --replace pontosan egy szakaszt vár (`script>…`, `template>…` vagy `style>…`)"
                )
                .to_owned(),
            ));
        }
        Ok(if program.script.is_some() {
            ("script", rendered.script.clone())
        } else if program.template.is_some() {
            ("template", rendered.template.clone())
        } else {
            ("style", rendered.style.clone())
        })
    }
}

/// A composable tagjai: csak script-tagok, props / emits nélkül.
fn composable_members(program: &Program) -> GenResult<&[Member]> {
    if program.template.is_some() || program.style.is_some() {
        return Err(GenError::new(
            crate::t!(
                "a .ts / .js file takes members only (no template / style): use a .vue file for those",
                "a .ts / .js fájl csak tagokat vár (template / style nélkül): azokhoz .vue fájl való"
            )
            .to_owned(),
        ));
    }
    let members = program
        .script
        .as_ref()
        .map_or(&[][..], |s| s.members.as_slice());
    if members.is_empty() {
        return Err(GenError::new(
            crate::t!(
                "the expression has no members (e.g. `comp:useCart > ref:items:list<Item>=[]`)",
                "a kifejezésben nincs tag (pl. `comp:useCart > ref:items:list<Item>=[]`)"
            )
            .to_owned(),
        ));
    }
    if members
        .iter()
        .any(|m| matches!(m, Member::Props(_) | Member::Emits(_)))
    {
        return Err(GenError::new(
            crate::t!(
                "props() / emits() belong to a <script setup>, not to a composable",
                "a props() / emits() <script setup>-ba való, nem composable-be"
            )
            .to_owned(),
        ));
    }
    Ok(members)
}

/// `--into x.vue`: a szakaszok beszúrása, majd az importok egyesítése. A generált kódsorokat adja vissza (`--dry`).
fn insert_vue(
    doc: &mut Doc,
    program: &Program,
    rendered: &Rendered,
    style: CodeStyle,
    after: Option<&str>,
) -> GenResult<Vec<String>> {
    let after_line = after.and_then(parse_line_ref);
    let after_name = after.filter(|_| after_line.is_none());
    if after_line.is_some() && program.template.is_none() {
        return Err(GenError::new(
            crate::t!(
                "--after L<n> places template elements: the expression needs a `template>` section",
                "az --after L<n> template-elemeket helyez el: a kifejezésben `template>` szakasz kell"
            )
            .to_owned(),
        ));
    }
    // a meglévő fájl behúzás-egysége (2 vagy 4 szóköz) szerint
    let unit = indent_unit(&doc.lines);
    let mut shown = Vec::new();
    if program.template.is_some() {
        let block = reindent(&rendered.template, unit);
        put_template(doc, &block, after_line)?;
        shown.extend(block);
    }
    if program.style.is_some() {
        let block = reindent(&rendered.style, unit);
        put_style(
            doc,
            &block,
            program.style.as_ref().is_some_and(|s| s.scoped),
        );
        shown.extend(block);
    }
    let members = program
        .script
        .as_ref()
        .map_or(&[][..], |s| s.members.as_slice());
    if !members.is_empty() {
        let block = reindent(&rendered.script, unit);
        put_script(doc, members, &block, style, after_name)?;
        shown.extend(block);
    }
    if !rendered.imports.is_empty() {
        put_imports(doc, &rendered.imports, style)?;
    }
    Ok(shown)
}

/// A kicserélendő `[from, to)` sortartomány (0-alapú) és a függvény neve, ha név alapján kerestük.
fn replace_range(
    doc: &Doc,
    kind: FileKind,
    at: &ReplaceAt,
    rel: &str,
) -> GenResult<(usize, usize, Option<String>)> {
    match at {
        ReplaceAt::Lines(a, b) => {
            if *a == 0 || a > b || *b > doc.lines.len() {
                return Err(GenError::new(crate::tf!(
                    "bad line range L{}-{} ({} has {} lines)",
                    "hibás sortartomány: L{}-{} ({} {} soros)",
                    a,
                    b,
                    rel,
                    doc.lines.len()
                )));
            }
            Ok((a - 1, *b, None))
        }
        ReplaceAt::Method { name, .. } => {
            let (lo, hi) = match kind {
                FileKind::Vue => {
                    let (tag, close) = block_range(doc, SfcBlockKind::ScriptSetup, "script")?
                        .ok_or_else(|| {
                            GenError::new(crate::tf!(
                                "no <script setup> in {}",
                                "nincs <script setup> ebben: {}",
                                rel
                            ))
                        })?;
                    (tag + 1, close)
                }
                FileKind::Script { .. } => (0, doc.lines.len()),
            };
            let (start, end) = find_statement(&doc.lines, name, lo, hi).ok_or_else(|| {
                GenError::new(crate::tf!(
                    "no `{}` in {} (declared: {})",
                    "nincs `{}` ebben: {} (deklarált: {})",
                    name,
                    rel,
                    declared_names(&doc.lines, lo, hi).join(", ")
                ))
            })?;
            Ok((start, end + 1, Some(name.clone())))
        }
    }
}

/// `--into x.ts`: tagok a `return {` elé, a visszaadott objektum bővítése, importok.
fn insert_script(
    doc: &mut Doc,
    program: &Program,
    rendered: &Rendered,
    style: CodeStyle,
    after: Option<&str>,
) -> GenResult<Vec<String>> {
    let members = composable_members(program)?;
    let em = ScriptEmitter {
        ts: style.ts,
        semi: style.semi,
    };
    let total = doc.lines.len();
    for name in members.iter().filter_map(Member::declared_name) {
        if find_statement(&doc.lines, name, 0, total).is_some() {
            return Err(GenError::new(crate::tf!(
                "`{}` is already declared in the file (to change it use --replace <file>::{})",
                "a(z) `{}` már deklarálva van a fájlban (módosításhoz: --replace <fájl>::{})",
                name,
                name
            )));
        }
    }
    let ret = find_return(&doc.lines).ok_or_else(|| {
        GenError::new(
            crate::t!(
                "no `return {` found: a composable file ends with `return { … }`",
                "nincs `return {` a fájlban: a composable-fájl `return { … }`-val végződik"
            )
            .to_owned(),
        )
    })?;
    let names: Vec<String> = members
        .iter()
        .filter_map(|m| m.declared_name().map(str::to_owned))
        .collect();
    extend_return(doc, ret, &names)?;
    let unit = indent_unit(&doc.lines);
    let block = indent_block(&reindent(&em.members(members), unit), unit);
    let at = match after {
        Some(name) => {
            let (_, end) = find_statement(&doc.lines, name, 0, ret).ok_or_else(|| {
                GenError::new(crate::tf!(
                    "no `{}` before the `return {{` (declared: {})",
                    "nincs `{}` a `return {{` előtt (deklarált: {})",
                    name,
                    declared_names(&doc.lines, 0, ret).join(", ")
                ))
            })?;
            end + 1
        }
        None => ret,
    };
    let mut inserted = Vec::new();
    let spaced = at > 0 && !doc.lines[at - 1].trim().is_empty();
    if spaced {
        inserted.push(String::new());
    }
    inserted.extend(block.iter().cloned());
    if doc.lines.get(at).is_some_and(|l| !l.trim().is_empty()) {
        inserted.push(String::new());
    }
    doc.insert(at, inserted);
    doc.mark("members", at + usize::from(spaced), block.len());
    if !rendered.imports.is_empty() {
        let end = doc.lines.len();
        for e in plan(&doc.lines, 0, end, &rendered.imports, style.semi) {
            doc.splice(e.start, e.end, e.lines);
        }
    }
    Ok(block)
}

/// A fájl már importált (lokális) nevei.
fn bound_names(lines: &[String]) -> Vec<String> {
    let mut names = Vec::new();
    for f in scan(lines, 0, lines.len()) {
        names.extend(f.import.default);
        names.extend(
            f.import
                .named
                .iter()
                .map(|n| n.rsplit(" as ").next().unwrap_or(n).trim().to_owned()),
        );
    }
    names
}

/// `L12` / `12` → 12.
fn parse_line_ref(text: &str) -> Option<usize> {
    text.trim()
        .trim_start_matches(['L', 'l'])
        .parse::<usize>()
        .ok()
        .filter(|n| *n > 0)
}

/// A blokk nyitó-tag sora és záró-tag sora (0-alapú); `None`, ha nincs ilyen blokk.
fn block_range(doc: &Doc, kind: SfcBlockKind, name: &str) -> GenResult<Option<(usize, usize)>> {
    let sfc = Sfc::parse(&doc.text());
    let Some(b) = sfc.blocks.iter().find(|b| b.kind == kind) else {
        return Ok(None);
    };
    let (tag, close) = (b.start_line as usize - 1, b.end_line as usize - 1);
    let closes_own_line = doc
        .lines
        .get(close)
        .is_some_and(|l| l.trim_start().starts_with("</"));
    if close <= tag || !closes_own_line {
        return Err(one_line_block(name));
    }
    Ok(Some((tag, close)))
}

fn put_script(
    doc: &mut Doc,
    members: &[Member],
    block: &[String],
    style: CodeStyle,
    after: Option<&str>,
) -> GenResult<()> {
    let range = block_range(doc, SfcBlockKind::ScriptSetup, "script")?;
    let Some((tag, close)) = range else {
        let mut ins = vec![script_open(style.ts).to_owned()];
        ins.extend(block.iter().cloned());
        ins.push("</script>".to_owned());
        ins.push(String::new());
        doc.insert(0, ins);
        doc.mark("script", 1, block.len());
        return Ok(());
    };
    let existing = doc.lines[tag + 1..close].join("\n");
    for m in members {
        let (macro_name, taken) = match m {
            Member::Props(_) => ("defineProps", existing.contains("defineProps")),
            Member::Emits(_) => ("defineEmits", existing.contains("defineEmits")),
            _ => ("", false),
        };
        if taken {
            return Err(GenError::new(crate::tf!(
                "the <script setup> already has {}",
                "a <script setup>-ban már van {}",
                macro_name
            )));
        }
        let clash = m
            .declared_name()
            .filter(|name| find_statement(&doc.lines, name, tag + 1, close).is_some());
        if let Some(name) = clash {
            return Err(GenError::new(crate::tf!(
                "`{}` is already declared in the <script setup> (to change it use --replace <file>::{})",
                "a(z) `{}` már deklarálva van a <script setup>-ban (módosításhoz: --replace <fájl>::{})",
                name,
                name
            )));
        }
    }
    let at = match after {
        Some(name) => {
            let (_, end) = find_statement(&doc.lines, name, tag + 1, close).ok_or_else(|| {
                GenError::new(crate::tf!(
                    "no `{}` in the <script setup> (declared: {})",
                    "nincs `{}` a <script setup>-ban (deklarált: {})",
                    name,
                    declared_names(&doc.lines, tag + 1, close).join(", ")
                ))
            })?;
            end + 1
        }
        None => last_nonblank(&doc.lines, tag + 1, close).map_or(tag + 1, |i| i + 1),
    };
    let spaced = at > tag + 1 && !doc.lines[at - 1].trim().is_empty();
    let mut ins = Vec::new();
    if spaced {
        ins.push(String::new());
    }
    ins.extend(block.iter().cloned());
    if after.is_some() && at < close && !doc.lines[at].trim().is_empty() {
        ins.push(String::new());
    }
    doc.insert(at, ins);
    doc.mark("script", at + usize::from(spaced), block.len());
    Ok(())
}

fn put_template(doc: &mut Doc, block: &[String], after: Option<usize>) -> GenResult<()> {
    let range = block_range(doc, SfcBlockKind::Template, "template")?;
    let Some((tag, close)) = range else {
        let mut tpl = vec!["<template>".to_owned()];
        tpl.extend(indent_block(block, 2));
        tpl.push("</template>".to_owned());
        let sfc = Sfc::parse(&doc.text());
        let scripts_end = sfc.scripts().map(|b| b.end_line as usize).max();
        match scripts_end {
            Some(end) => {
                let mut ins = vec![String::new()];
                ins.extend(tpl);
                let len = block.len();
                doc.insert(end, ins);
                doc.mark("template", end + 2, len);
            }
            None => {
                tpl.push(String::new());
                doc.insert(0, tpl);
                doc.mark("template", 1, block.len());
            }
        }
        return Ok(());
    };
    let (at, indent) = match after {
        Some(n) => {
            if n <= tag || n > close {
                return Err(GenError::new(crate::tf!(
                    "--after L{} is outside the <template> (L{}-L{})",
                    "az --after L{} a <template>-en kívül van (L{}-L{})",
                    n,
                    tag + 1,
                    close + 1
                )));
            }
            let line = &doc.lines[n - 1];
            (n, indent_of(line) + if opens_element(line) { 2 } else { 0 })
        }
        None => match last_nonblank(&doc.lines, tag + 1, close) {
            None => (tag + 1, 2),
            Some(last) => {
                let line = &doc.lines[last];
                if line.trim_start().starts_with("</") {
                    (last, indent_of(line) + 2)
                } else {
                    (last + 1, indent_of(line))
                }
            }
        },
    };
    let ins = indent_block(block, indent);
    let len = ins.len();
    doc.insert(at, ins);
    doc.mark("template", at, len);
    Ok(())
}

fn put_style(doc: &mut Doc, block: &[String], scoped: bool) {
    let range = block_range(doc, SfcBlockKind::Style, "style")
        .ok()
        .flatten();
    let Some((tag, close)) = range else {
        while doc.lines.last().is_some_and(|l| l.trim().is_empty()) {
            doc.lines.pop();
        }
        if !doc.lines.is_empty() {
            doc.lines.push(String::new());
        }
        let start = doc.lines.len() + 1;
        doc.lines.push(style_open(scoped).to_owned());
        doc.lines.extend(block.iter().cloned());
        doc.lines.push("</style>".to_owned());
        doc.lines.push(String::new());
        doc.mark("style", start, block.len());
        return;
    };
    let last = last_nonblank(&doc.lines, tag + 1, close);
    let at = last.map_or(tag + 1, |i| i + 1);
    let mut ins = Vec::new();
    if last.is_some() {
        ins.push(String::new());
    }
    ins.extend(block.iter().cloned());
    doc.insert(at, ins);
    doc.mark("style", at + usize::from(last.is_some()), block.len());
}

fn put_imports(doc: &mut Doc, imports: &ImportSet, style: CodeStyle) -> GenResult<()> {
    let range = block_range(doc, SfcBlockKind::ScriptSetup, "script")?;
    let Some((tag, close)) = range else {
        let mut ins = vec![script_open(style.ts).to_owned()];
        ins.extend(imports.lines(style.semi));
        ins.push("</script>".to_owned());
        ins.push(String::new());
        doc.insert(0, ins);
        return Ok(());
    };
    for e in plan(&doc.lines, tag + 1, close, imports, style.semi) {
        doc.splice(e.start, e.end, e.lines);
    }
    Ok(())
}

/// A composable `return {` sora: a legkisebb behúzású utolsó ilyen sor.
fn find_return(lines: &[String]) -> Option<usize> {
    let candidates: Vec<usize> = (0..lines.len())
        .filter(|&i| lines[i].trim_start().starts_with("return {"))
        .collect();
    let min = candidates.iter().map(|&i| indent_of(&lines[i])).min()?;
    candidates
        .into_iter()
        .rfind(|&i| indent_of(&lines[i]) == min)
}

/// A visszaadott objektum bővítése az új nevekkel (egy- és többsoros alak).
fn extend_return(doc: &mut Doc, ret: usize, names: &[String]) -> GenResult<()> {
    let line = doc.lines[ret].clone();
    let open = line.find('{').unwrap_or(0);
    if let Some(rel_close) = line[open..].rfind('}').filter(|&r| r > 0) {
        let inner = &line[open + 1..open + rel_close];
        let existing: Vec<&str> = inner.split(',').map(str::trim).collect();
        let add: Vec<&String> = names
            .iter()
            .filter(|n| !existing.contains(&n.as_str()))
            .collect();
        if add.is_empty() {
            return Ok(());
        }
        let added = add
            .iter()
            .map(|n| n.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let inner = inner.trim().trim_end_matches(',').trim();
        let merged = if inner.is_empty() {
            added
        } else {
            format!("{inner}, {added}")
        };
        doc.lines[ret] = format!("{}{{ {merged} {}", &line[..open], &line[open + rel_close..]);
        return Ok(());
    }
    let close = (ret + 1..doc.lines.len())
        .find(|&j| doc.lines[j].trim_start().starts_with('}'))
        .ok_or_else(|| {
            GenError::new(
                crate::t!(
                    "the `return {` object is not closed",
                    "a `return {` objektum nincs lezárva"
                )
                .to_owned(),
            )
        })?;
    let existing: Vec<String> = doc.lines[ret + 1..close]
        .iter()
        .map(|l| {
            let t = l.trim().trim_end_matches(',');
            t.split(':').next().unwrap_or(t).trim().to_owned()
        })
        .collect();
    let add: Vec<&String> = names.iter().filter(|n| !existing.contains(n)).collect();
    if add.is_empty() {
        return Ok(());
    }
    let pad = " ".repeat(if close > ret + 1 {
        indent_of(&doc.lines[ret + 1])
    } else {
        indent_of(&doc.lines[ret]) + 2
    });
    if close > ret + 1 && !doc.lines[close - 1].trim_end().ends_with(',') {
        doc.lines[close - 1].push(',');
    }
    let new: Vec<String> = add.iter().map(|n| format!("{pad}{n},")).collect();
    doc.insert(close, new);
    Ok(())
}
