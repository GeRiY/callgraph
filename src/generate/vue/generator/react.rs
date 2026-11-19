//! React: the new `.tsx` / `.jsx` component and the `.ts` / `.js` hook (`--write`, preview), and `--into` a
//! component (state / handlers before the `return`, elements before the root's closing tag, props into the
//! signature and the props interface).

use super::super::builder::composable_file;
use super::super::edit::{Doc, find_statement, indent_block, indent_of, indent_unit, reindent};
use super::super::imports::{ImportSet, plan};
use super::super::model::{Element, Import, Kind, Member, Program};
use super::super::project::CodeStyle;
use super::super::react::{PropDecl, ReactEmitter, needed_imports, props_of, setter_name};
use super::super::template::components;
use super::sfc::parse_line_ref;
use super::{VueGenerator, file_kind};
use crate::generate::command::{EditCommand, WriteNew};
use crate::generate::error::{GenError, GenResult};
use crate::generate::generator::{Mode, Output};
use crate::generate::head::is_ident;
use crate::generate::target::Loaded;

/// Inertia's React tags.
const INERTIA_TAGS: [&str; 2] = ["Link", "Head"];

/// The file stem of `rel` (`resources/js/Components/Card.tsx` -> `Card`).
fn stem_of(rel: &str) -> &str {
    let file = rel.rsplit('/').next().unwrap_or(rel);
    file.rsplit_once('.').map_or(file, |(s, _)| s)
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().collect::<String>() + chars.as_str()
    })
}

fn members_of(program: &Program) -> &[Member] {
    program
        .script
        .as_ref()
        .map_or(&[][..], |s| s.members.as_slice())
}

/// Is the line the start of a `return (` / `return <` of a component.
fn is_return(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("return (") || t.starts_with("return <")
}

/// The component's final `return`: the least indented (but indented) such line, the last one.
fn find_jsx_return(lines: &[String]) -> Option<usize> {
    let candidates: Vec<usize> = (0..lines.len())
        .filter(|&i| is_return(&lines[i]) && indent_of(&lines[i]) > 0)
        .collect();
    let min = candidates.iter().map(|&i| indent_of(&lines[i])).min()?;
    candidates
        .into_iter()
        .rfind(|&i| indent_of(&lines[i]) == min)
}

/// Does `line` bind `name` as a whole word in an array destructuring (`const [count, setCount] = useState`).
fn binds_state(line: &str, name: &str) -> bool {
    let t = line.trim_start();
    if !t.starts_with("const [") {
        return false;
    }
    let inner = t["const [".len()..].split(']').next().unwrap_or("");
    inner.split(',').any(|n| n.trim() == name)
}

fn bad_shape(what: &str) -> GenError {
    GenError::new(crate::tf!(
        "cannot find {} in the component: edit it by hand",
        "nem találom a komponensben: {} — szerkeszd kézzel",
        what
    ))
}

impl VueGenerator<'_> {
    /// The component imports of a JSX tree: the project's `<Name>.tsx` / `.jsx`, Inertia's `Link` / `Head`.
    fn jsx_component_imports(
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
            if let Some((source, named)) = project.find_jsx_import(&name) {
                imports.push(if named {
                    Import {
                        source,
                        default: None,
                        named: vec![name],
                    }
                } else {
                    Import {
                        source,
                        default: Some(name),
                        named: Vec::new(),
                    }
                });
            } else if INERTIA_TAGS.contains(&name.as_str()) {
                imports.push(Import {
                    source: "@inertiajs/react".to_owned(),
                    default: None,
                    named: vec![name],
                });
            } else {
                unresolved.push(name);
            }
        }
        (imports, unresolved)
    }

    /// The imports of a React program: explicit `use:` (a Vue `.vue` suffix is dropped), the members' needs, the
    /// components of the JSX; plus the names the page already binds.
    fn react_imports(
        &self,
        program: &Program,
        ts: bool,
        bound: &[String],
    ) -> (ImportSet, Vec<String>) {
        let mut set = ImportSet::new();
        let mut unresolved = Vec::new();
        let mut explicit: Vec<Import> = Vec::new();
        if let Some(s) = &program.script {
            explicit = self
                .resolve_uses(&s.imports, &s.guess)
                .into_iter()
                .map(|mut i| {
                    if let Some(p) = i.source.strip_suffix(".vue") {
                        i.source = p.to_owned();
                    }
                    i
                })
                .collect();
            set.extend(explicit.iter().cloned());
            set.extend(needed_imports(&s.members, ts));
        }
        if let Some(elements) = &program.template {
            let (found, missing) = self.jsx_component_imports(elements, &explicit, bound);
            set.extend(found);
            unresolved = missing;
        }
        (set, unresolved)
    }

    fn react_name(program: &Program, rel: &str) -> GenResult<String> {
        let name = program.name.clone().unwrap_or_else(|| {
            if rel == "preview" {
                "Preview".to_owned()
            } else {
                capitalized(stem_of(rel))
            }
        });
        if is_ident(&name) && name.starts_with(char::is_uppercase) {
            Ok(name)
        } else {
            Err(GenError::new(crate::tf!(
                "a component name starts with a capital letter: `react:Card > …` (got `{}`)",
                "a komponens neve nagybetűvel kezdődik: `react:Card > …` (ez jött: `{}`)",
                name
            )))
        }
    }

    fn check_component(program: &Program) -> GenResult<()> {
        if program.style.is_some() {
            return Err(GenError::new(
                crate::t!(
                    "React has no style section: use className (or a CSS file)",
                    "a Reactben nincs style szakasz: használj className-et (vagy CSS-fájlt)"
                )
                .to_owned(),
            ));
        }
        if members_of(program)
            .iter()
            .any(|m| matches!(m, Member::Class(_)))
        {
            return Err(GenError::new(
                crate::t!(
                    "a component takes props / ref / fn / const / on: members, not a class",
                    "a komponens props / ref / fn / const / on: tagokat vár, nem osztályt"
                )
                .to_owned(),
            ));
        }
        Ok(())
    }

    /// The full text of a new component; also the preset's page text (`unit`: the project's indent unit).
    pub(crate) fn react_text(
        &self,
        program: &Program,
        name: &str,
        style: CodeStyle,
        unit: usize,
        default_export: bool,
    ) -> GenResult<(String, Vec<String>)> {
        Self::check_component(program)?;
        let (imports, unresolved) = self.react_imports(program, style.ts, &[]);
        let em = ReactEmitter {
            ts: style.ts,
            semi: style.semi,
        };
        let component = em.component(
            name,
            members_of(program),
            program.template.as_deref(),
            default_export,
        )?;
        let text = composable_file(&imports.lines(style.semi), &component);
        let lines: Vec<String> = text.lines().map(str::to_owned).collect();
        let mut out = reindent(&lines, unit).join("\n");
        out.push('\n');
        Ok((out, unresolved))
    }

    /// The full text of a new custom hook.
    fn hook_text(&self, program: &Program, rel: &str, style: CodeStyle) -> GenResult<String> {
        let name = match &program.name {
            Some(n) => n.clone(),
            None => {
                let stem = stem_of(rel);
                if is_ident(stem) {
                    stem.to_owned()
                } else {
                    return Err(GenError::new(
                        crate::t!(
                            "name the hook: `hook:useCart > …`",
                            "nevezd el a hookot: `hook:useCart > …`"
                        )
                        .to_owned(),
                    ));
                }
            }
        };
        if program.template.is_some() || program.style.is_some() {
            return Err(GenError::new(
                crate::t!(
                    "a hook has no template / style: members only",
                    "a hooknak nincs template / style szakasza: csak tagok"
                )
                .to_owned(),
            ));
        }
        let members = members_of(program);
        if members.is_empty() {
            return Err(GenError::new(
                crate::t!(
                    "the expression has no members (e.g. `hook:useCart > ref:items:list<Item>=[]`)",
                    "a kifejezésben nincs tag (pl. `hook:useCart > ref:items:list<Item>=[]`)"
                )
                .to_owned(),
            ));
        }
        if members
            .iter()
            .any(|m| matches!(m, Member::Props(_) | Member::Emits(_) | Member::Class(_)))
        {
            return Err(GenError::new(
                crate::t!(
                    "props() / emits() belong to a component, not to a hook",
                    "a props() / emits() komponensbe való, nem hookba"
                )
                .to_owned(),
            ));
        }
        let (imports, _) = self.react_imports(program, style.ts, &[]);
        let em = ReactEmitter {
            ts: style.ts,
            semi: style.semi,
        };
        Ok(composable_file(
            &imports.lines(style.semi),
            &em.hook(&name, members),
        ))
    }

    /// The preview (no target): a component or a hook in the project's style.
    pub(super) fn preview_react(&self, program: &Program) -> GenResult<Output> {
        let project = self.project();
        let style = project.jsx_code_style();
        if program.kind == Kind::Hook {
            let text = self.hook_text(program, "", style)?;
            self.validate(if style.ts { "preview.ts" } else { "preview.js" }, &text)?;
            return Ok(Output {
                lines: text.lines().map(str::to_owned).collect(),
                unresolved: Vec::new(),
            });
        }
        let name = Self::react_name(program, "preview")?;
        let (text, unresolved) = self.react_text(
            program,
            &name,
            style,
            project.jsx_indent(),
            project.default_exports(),
        )?;
        self.validate(
            if style.ts {
                "preview.tsx"
            } else {
                "preview.jsx"
            },
            &text,
        )?;
        Ok(Output {
            lines: text.lines().map(str::to_owned).collect(),
            unresolved,
        })
    }

    /// `--write x.tsx` / `x.jsx` (a component) or `x.ts` / `x.js` with a `hook:` head.
    pub(super) fn write_react(
        &self,
        program: &Program,
        rel: String,
        ts: bool,
        mode: Mode,
    ) -> GenResult<Output> {
        let project = self.project();
        let style = CodeStyle {
            ts,
            semi: project.jsx_code_style().semi,
        };
        let (text, unresolved) = if program.kind == Kind::Hook {
            (self.hook_text(program, &rel, style)?, Vec::new())
        } else {
            let name = Self::react_name(program, &rel)?;
            self.react_text(
                program,
                &name,
                style,
                project.jsx_indent(),
                project.default_exports(),
            )?
        };
        self.validate(&rel, &text)?;
        if mode.dry() {
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

    /// `--into x.tsx`: state / handlers before the final `return`, JSX elements before the root's closing tag,
    /// props into the signature and the props interface.
    pub(super) fn insert_react(
        &self,
        program: &Program,
        file: &str,
        after: Option<&str>,
        mode: Mode,
    ) -> GenResult<Output> {
        let rel = self.rel(file)?;
        let kind = file_kind(&rel)?;
        let Loaded { text, lines, eol } = self.load(&rel)?;
        let style = self.style_of(kind, &text);
        Self::check_component(program)?;
        let mut doc = Doc::new(lines);
        let (wanted, unresolved) =
            self.react_imports(program, style.ts, &super::sfc::bound_names(&doc.lines));
        let em = ReactEmitter {
            ts: style.ts,
            semi: style.semi,
        };
        let unit = indent_unit(&doc.lines);
        let members = members_of(program);
        let body: Vec<&Member> = members
            .iter()
            .filter(|m| !matches!(m, Member::Props(_) | Member::Emits(_)))
            .collect();
        let ret = find_jsx_return(&doc.lines).ok_or_else(|| bad_shape("the final `return (`"))?;
        let mut blocks: Vec<Vec<String>> = Vec::new();

        // 1. elements: before the root's closing tag (or after `--after L<n>`)
        if let Some(elements) = program.template.as_deref() {
            let at = self.element_slot(&doc, ret, after)?;
            let pad = indent_of(&doc.lines[ret]) + unit * 2;
            let block = indent_block(&reindent(&em.elements(elements)?, unit), pad);
            doc.insert(at, block.clone());
            doc.mark("jsx", at, block.len());
            blocks.push(block);
        }

        // 2. members: before the `return` (or after `--after <name>`)
        if !body.is_empty() {
            self.check_new_names(&doc, &body, ret)?;
            let at = match after.filter(|a| parse_line_ref(a).is_none()) {
                Some(name) => {
                    let (_, end) = find_statement(&doc.lines, name, 0, ret).ok_or_else(|| {
                        GenError::new(crate::tf!(
                            "no `{}` before the `return` of the component",
                            "nincs `{}` a komponens `return`-je előtt",
                            name
                        ))
                    })?;
                    end + 1
                }
                None => ret,
            };
            let pad = indent_of(&doc.lines[ret]);
            let block = indent_block(&reindent(&em.members(&body), unit), pad);
            let spaced = at > 0 && !doc.lines[at - 1].trim().is_empty();
            let mut inserted = Vec::new();
            if spaced {
                inserted.push(String::new());
            }
            inserted.extend(block.iter().cloned());
            if doc.lines.get(at).is_some_and(|l| !l.trim().is_empty()) {
                inserted.push(String::new());
            }
            doc.insert(at, inserted);
            doc.mark("members", at + usize::from(spaced), block.len());
            blocks.push(block);
        }

        // 3. props: the signature and the interface (the earliest lines of the file, so last)
        let props = props_of(members);
        if !props.is_empty() {
            let block = add_props(&mut doc, &props, style.ts, unit)?;
            blocks.push(block);
        }

        if !wanted.is_empty() {
            let end = doc.lines.len();
            for e in plan(&doc.lines, 0, end, &wanted, style.semi) {
                doc.splice(e.start, e.end, e.lines);
            }
        }
        let shown: Vec<String> = blocks.into_iter().flatten().collect();
        self.commit_insert(rel, &doc, eol, shown, unresolved, mode)
    }

    /// The line index where new elements go: before the root element's closing tag, or after `L<n>`.
    fn element_slot(&self, doc: &Doc, ret: usize, after: Option<&str>) -> GenResult<usize> {
        let indent = indent_of(&doc.lines[ret]);
        let close = (ret + 1..doc.lines.len())
            .find(|&j| {
                indent_of(&doc.lines[j]) == indent && doc.lines[j].trim_start().starts_with(')')
            })
            .filter(|_| doc.lines[ret].trim() == "return (")
            .ok_or_else(|| bad_shape("`return (` … `)` around the JSX"))?;
        let root_close = close - 1;
        if root_close <= ret + 1 || !doc.lines[root_close].trim_start().starts_with("</") {
            return Err(bad_shape(
                "a root element with its closing tag on its own line",
            ));
        }
        match after.and_then(parse_line_ref) {
            Some(n) if n > ret + 1 && n <= root_close => Ok(n),
            Some(n) => Err(GenError::new(crate::tf!(
                "L{} is outside the JSX of the component (L{}-L{})",
                "az L{} a komponens JSX-én kívül van (L{}-L{})",
                n,
                ret + 2,
                root_close
            ))),
            None => Ok(root_close),
        }
    }

    /// A name the new members declare must not be declared before the `return` already.
    fn check_new_names(&self, doc: &Doc, body: &[&Member], ret: usize) -> GenResult<()> {
        for m in body {
            for name in m.declares() {
                let setter = matches!(m, Member::Ref { .. }).then(|| setter_name(name));
                let taken = find_statement(&doc.lines, name, 0, ret).is_some()
                    || doc.lines[..ret].iter().any(|l| {
                        binds_state(l, name) || setter.as_deref().is_some_and(|s| binds_state(l, s))
                    });
                if taken {
                    return Err(GenError::new(crate::tf!(
                        "`{}` is already declared in the file (to change it edit it by hand)",
                        "a(z) `{}` már deklarálva van a fájlban (módosításhoz szerkeszd kézzel)",
                        name
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Add props to the component's destructured parameter and (TS) to its props interface; returns the shown lines.
fn add_props(doc: &mut Doc, props: &[PropDecl], ts: bool, unit: usize) -> GenResult<Vec<String>> {
    let sig = (0..doc.lines.len())
        .find(|&i| {
            let t = doc.lines[i].trim_start();
            let t = t.strip_prefix("export ").unwrap_or(t);
            let t = t.strip_prefix("default ").unwrap_or(t);
            t.starts_with("function ")
                && t["function ".len()..].starts_with(char::is_uppercase)
                && doc.lines[i].contains("({")
        })
        .ok_or_else(|| bad_shape("`function Name({ … })` with destructured props"))?;
    let line = doc.lines[sig].clone();
    let open = line.find("({").map_or(0, |i| i + 1);
    let close = line[open..]
        .find('}')
        .map(|i| open + i)
        .ok_or_else(|| bad_shape("the props destructuring on one line"))?;
    let existing: Vec<String> = line[open + 1..close]
        .split(',')
        .map(|p| p.split('=').next().unwrap_or("").trim().to_owned())
        .collect();
    if let Some(p) = props.iter().find(|p| existing.contains(&p.name)) {
        return Err(GenError::new(crate::tf!(
            "`{}` is already a prop of the component",
            "a(z) `{}` már prop a komponensben",
            p.name
        )));
    }
    let added: Vec<String> = props
        .iter()
        .map(|p| match &p.default {
            Some(d) => format!("{} = {d}", p.name),
            None => p.name.clone(),
        })
        .collect();
    let inner = line[open + 1..close].trim();
    let merged = if inner.is_empty() {
        added.join(", ")
    } else {
        format!("{inner}, {}", added.join(", "))
    };
    let mut shown = vec![format!("props: {}", added.join(", "))];
    let annotation = line[close + 1..].trim_start().strip_prefix(':').map(|t| {
        t.trim_start()
            .split([')', ' '])
            .next()
            .unwrap_or("")
            .to_owned()
    });
    doc.lines[sig] = format!("{}{{ {merged} {}", &line[..open], &line[close..]);
    if ts {
        let name = annotation
            .filter(|n| is_ident(n))
            .ok_or_else(|| bad_shape("a named props type: `}: CardProps`"))?;
        let decl = (0..doc.lines.len())
            .find(|&i| {
                let t = doc.lines[i].trim_start();
                let t = t.strip_prefix("export ").unwrap_or(t);
                t.starts_with(&format!("interface {name} "))
                    || t.starts_with(&format!("interface {name}{{"))
                    || (t.starts_with(&format!("type {name} ")) && t.trim_end().ends_with('{'))
            })
            .ok_or_else(|| bad_shape(&format!("`interface {name}`")))?;
        let pad = indent_of(&doc.lines[decl]);
        let end = (decl + 1..doc.lines.len())
            .find(|&j| {
                indent_of(&doc.lines[j]) == pad && doc.lines[j].trim_start().starts_with('}')
            })
            .ok_or_else(|| bad_shape(&format!("the end of `{name}`")))?;
        let semi = doc.lines[decl + 1..end]
            .iter()
            .any(|l| l.trim_end().ends_with(';'));
        let rows: Vec<String> = props
            .iter()
            .map(|p| {
                let opt = if p.optional { "?" } else { "" };
                format!(
                    "{}{}{opt}: {}{}",
                    " ".repeat(pad + unit),
                    p.name,
                    p.ty,
                    if semi || end == decl + 1 { ";" } else { "" }
                )
            })
            .collect();
        shown.extend(rows.iter().map(|r| r.trim().to_owned()));
        doc.insert(end, rows);
        doc.mark("props", end, props.len());
    }
    Ok(shown)
}
