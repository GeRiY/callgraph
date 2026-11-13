//! Facade: a `Generator` az egyetlen belépő a CLI-nek és az MCP-nek. Összefogja a lépéseket: kifejezés → AST
//! (Interpreter) → projekt-ismeret (névtér, stílus, importok a feloldó-láncból) → emitter (Strategy) → fájl
//! (Builder) → ellenőrzés (a nyelv parserével) → parancs (Command: írás mentéssel). Semmi nem íródik, amíg a teljes
//! új szöveg nem parse-ol hibátlanul.

use std::collections::BTreeSet;
use std::path::{Component, Path};

use super::ast::{Member, Program, TypeKind};
use super::command::{EditCommand, InsertInto, ReplaceRange, Undo, WriteNew};
use super::edit::{
    add_imports, extend_over_docs, insert_after_line, insert_at_end, join_lines, range_text,
    split_lines,
};
use super::emitter::{Emitter, FileSpec};
use super::error::{GenError, GenResult};
use super::fs::Env;
use super::parser;
use super::project::{Psr4, detect_strict_types};
use super::resolve::ResolverChain;
use crate::model::{Container, FileGraph};

/// Hová menjen a generált kód.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Csak kiírás (`--dry`, a kód a kimenet).
    Preview,
    /// Új fájl (létezőt nem ír felül).
    Write { file: String },
    /// Tagok beszúrása egy osztályba.
    Into {
        file: String,
        class: Option<String>,
        after: Option<String>,
    },
    /// Egy tartomány / metódus cseréje.
    Replace {
        file: String,
        at: ReplaceAt,
        expect: Option<String>,
    },
}

/// Mit cseréljen a `Target::Replace`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplaceAt {
    /// Sortartomány (1-alapú, zárt).
    Lines(usize, usize),
    /// Egy metódus (a docblockjával és az attribútumaival); `class`: ha több osztályban is van ilyen.
    Method { class: Option<String>, name: String },
}

/// Egy generálási kérés.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenRequest {
    pub expr: String,
    /// Csak kiírás: semmi nem íródik, a kimenet a generált kód.
    pub dry: bool,
    pub target: Target,
}

/// A parancs kimenete: az egy-egy soros jelentés (`--dry`-nál a kód) és a feloldatlan nevek.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Output {
    pub lines: Vec<String>,
    pub unresolved: Vec<String>,
}

impl Output {
    /// A kiírandó szöveg (záró újsorral).
    pub fn render(&self) -> String {
        let mut text = self.lines.join("\n");
        text.push('\n');
        if !self.unresolved.is_empty() {
            text.push_str(&format!("unresolved: {}\n", self.unresolved.join(", ")));
        }
        text
    }
}

/// Egy beolvasott, elemzett meglévő fájl.
struct Loaded {
    lines: Vec<String>,
    eol: &'static str,
    graph: FileGraph,
}

/// A Facade.
pub struct Generator<'a, E: Emitter> {
    env: Env<'a>,
    emitter: E,
}

/// A fájl `alias → FQCN` importjai.
fn file_imports(graph: &FileGraph) -> Vec<(String, String)> {
    graph
        .imports
        .parts()
        .0
        .iter()
        .map(|(alias, full)| (alias.clone(), full.trim_start_matches('\\').to_owned()))
        .collect()
}

fn last_segment(fqcn: &str) -> &str {
    fqcn.rsplit('\\').next().unwrap_or(fqcn)
}

impl<'a, E: Emitter> Generator<'a, E> {
    pub fn new(env: Env<'a>, emitter: E) -> Self {
        Self { env, emitter }
    }

    /// A kérés végrehajtása.
    pub fn run(&self, request: &GenRequest) -> GenResult<Output> {
        let program = parser::parse(&request.expr)?;
        match &request.target {
            Target::Preview => self.preview(&program),
            Target::Write { file } => self.write(&program, file, request.dry),
            Target::Into { file, class, after } => self.insert_members(
                &program,
                file,
                class.as_deref(),
                after.as_deref(),
                request.dry,
            ),
            Target::Replace { file, at, expect } => {
                self.replace(&program, file, at, expect.as_deref(), request.dry)
            }
        }
    }

    /// Az `undo`: a fájl (vagy a legutóbb mentett fájl) legutóbbi mentésének visszaállítása.
    pub fn undo(&self, file: Option<&str>) -> GenResult<Output> {
        let rel = file.map(|f| self.rel(f)).transpose()?;
        let line = Undo { rel }.execute(self.env)?;
        Ok(Output {
            lines: vec![line],
            unresolved: Vec::new(),
        })
    }

    /// A felhasználói fájl-út projekt-relatív, `/`-es alakja; a gyökéren kívüli út hiba.
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

    fn check_extension(&self, rel: &str) -> GenResult<()> {
        let extension = self.emitter.extension();
        if rel.ends_with(&format!(".{extension}")) {
            Ok(())
        } else {
            Err(GenError::new(crate::tf!(
                "{} should end in .{}",
                "{} kiterjesztése .{} legyen",
                rel,
                extension
            )))
        }
    }

    /// A kész szöveg ellenőrzése a nyelv parserével: hibánál semmi nem íródik.
    fn validate(&self, rel: &str, text: &str) -> GenResult<()> {
        self.emitter.check(rel, text).map_err(|(line, message)| {
            GenError::new(crate::tf!(
                "syntax error at L{}: {} (nothing written)",
                "szintaktikai hiba az L{}. sorban: {} (nem írtam semmit)",
                line,
                message
            ))
        })
    }

    /// A rövid nevek feloldása (Chain of Responsibility): az importálandók és a feloldatlan nevek.
    fn resolve_names(
        &self,
        names: &BTreeSet<String>,
        program: &Program,
        namespace: Option<&str>,
        existing: &[(String, String)],
        psr4: &Psr4,
    ) -> (Vec<(String, Option<String>)>, Vec<String>) {
        let alias_of = |fqcn: &str, alias: &Option<String>| {
            alias
                .clone()
                .unwrap_or_else(|| last_segment(fqcn).to_owned())
        };
        let mut explicit: Vec<(String, String)> = program
            .uses
            .iter()
            .map(|u| (alias_of(&u.fqcn, &u.alias), u.fqcn.clone()))
            .collect();
        explicit.extend(existing.iter().cloned());
        let chain = ResolverChain::php(explicit.clone(), self.env.fs, self.env.root, psr4);
        let mut imports: Vec<(String, Option<String>)> = program
            .uses
            .iter()
            .map(|u| {
                let alias = u
                    .alias
                    .clone()
                    .filter(|a| a.as_str() != last_segment(&u.fqcn));
                (u.fqcn.clone(), alias)
            })
            .collect();
        let mut unresolved = Vec::new();
        let home = namespace.unwrap_or("");
        for name in names {
            if explicit.iter().any(|(alias, _)| alias == name) {
                continue;
            }
            match chain.resolve(name) {
                Some(fqcn) => {
                    let ns = fqcn.rsplit_once('\\').map_or("", |(ns, _)| ns);
                    if ns != home {
                        imports.push((fqcn, None));
                    }
                }
                None => unresolved.push(name.clone()),
            }
        }
        (imports, unresolved)
    }

    /// Csupa deklarációs kifejezés teljes fájllá; `rel`: a cél (névtér a PSR-4-ből).
    fn render_file(
        &self,
        program: &Program,
        rel: Option<&str>,
    ) -> GenResult<(String, Vec<String>)> {
        let psr4 = Psr4::load(self.env.fs, self.env.root);
        let namespace = program
            .namespace
            .clone()
            .or_else(|| rel.and_then(|r| psr4.namespace_of(r)));
        let names = self.emitter.referenced_names(&program.decls, &[]);
        let (imports, unresolved) =
            self.resolve_names(&names, program, namespace.as_deref(), &[], &psr4);
        let strict_types = detect_strict_types(self.env.fs, self.env.root, &psr4);
        let spec = FileSpec {
            namespace: namespace.as_deref(),
            imports: &imports,
            strict_types,
        };
        Ok((self.emitter.file(&spec, &program.decls), unresolved))
    }

    fn preview(&self, program: &Program) -> GenResult<Output> {
        if !program.decls.is_empty() {
            let (text, unresolved) = self.render_file(program, None)?;
            self.validate("preview", &text)?;
            return Ok(Output {
                lines: text.lines().map(str::to_owned).collect(),
                unresolved,
            });
        }
        let block: Vec<String> = self
            .emitter
            .members(TypeKind::Class, &program.loose)
            .collect();
        // ellenőrzés egy ideiglenes osztályba csomagolva (3 fejléc-sor)
        let wrapped = format!("<?php\nclass Generated\n{{\n{}\n}}\n", block.join("\n"));
        self.emitter
            .check("preview", &wrapped)
            .map_err(|(line, message)| {
                GenError::new(crate::tf!(
                    "syntax error at L{}: {}",
                    "szintaktikai hiba az L{}. sorban: {}",
                    line.saturating_sub(3).max(1),
                    message
                ))
            })?;
        let names = self.emitter.referenced_names(&[], &program.loose);
        let psr4 = Psr4::load(self.env.fs, self.env.root);
        let (_, unresolved) = self.resolve_names(&names, program, None, &[], &psr4);
        Ok(Output {
            lines: block,
            unresolved,
        })
    }

    fn write(&self, program: &Program, file: &str, dry: bool) -> GenResult<Output> {
        if program.decls.is_empty() {
            return Err(GenError::new(
                crate::t!(
                    "--write creates a whole file, so the expression needs a type (e.g. `c:Name > +run():void`); for members use --into",
                    "a --write teljes fájlt hoz létre, ezért típus kell a kifejezésbe (pl. `c:Név > +run():void`); tagokhoz --into való"
                )
                .to_owned(),
            ));
        }
        let rel = self.rel(file)?;
        self.check_extension(&rel)?;
        let (text, unresolved) = self.render_file(program, Some(&rel))?;
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

    /// A meglévő fájl beolvasása és elemzése.
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
        let graph = self.emitter.analyze(rel, &text);
        Ok(Loaded { lines, eol, graph })
    }

    /// A tagok forrása a `--into`-hoz: egyetlen típus tagjai, vagy a csupasz tagok.
    fn member_source<'p>(&self, program: &'p Program) -> GenResult<&'p [Member]> {
        let members: &[Member] = match (program.decls.as_slice(), program.loose.is_empty()) {
            ([decl], true) => &decl.members,
            ([], false) => &program.loose,
            _ => {
                return Err(GenError::new(
                    crate::t!(
                        "--into takes one type (its members are inserted) or bare members, e.g. `+charge(Order $o):Receipt{ … }`",
                        "az --into egy típust (a tagjait szúrja be) vagy csupasz tagokat vár, pl. `+charge(Order $o):Receipt{ … }`"
                    )
                    .to_owned(),
                ));
            }
        };
        if members.is_empty() {
            return Err(GenError::new(
                crate::t!(
                    "the expression has no members to insert",
                    "a kifejezésben nincs beszúrandó tag"
                )
                .to_owned(),
            ));
        }
        Ok(members)
    }

    fn insert_members(
        &self,
        program: &Program,
        file: &str,
        class: Option<&str>,
        after: Option<&str>,
        dry: bool,
    ) -> GenResult<Output> {
        let rel = self.rel(file)?;
        self.check_extension(&rel)?;
        let members = self.member_source(program)?;
        let Loaded {
            mut lines,
            eol,
            graph,
        } = self.load(&rel)?;
        let container = pick_container(&graph, class, &rel)?;
        let kind = TypeKind::from_graph(&container.kind);
        let block: Vec<String> = self.emitter.members(kind, members).collect();
        let names = self.emitter.referenced_names(&[], members);
        let psr4 = Psr4::load(self.env.fs, self.env.root);
        let (imports, unresolved) = self.resolve_names(
            &names,
            program,
            graph.namespace.as_deref(),
            &file_imports(&graph),
            &psr4,
        );
        let first = match after {
            Some(method) => {
                let end = find_method(&graph, container, method)?;
                insert_after_line(&mut lines, end, &block)
            }
            None => {
                if container.line >= container.end_line {
                    return Err(GenError::new(crate::tf!(
                        "{} is declared on one line; reformat it so its `{{` and `}}` are on separate lines",
                        "a(z) {} egy sorban van deklarálva; tördeld úgy, hogy a `{{` és a `}}` külön sorban legyen",
                        container.name
                    )));
                }
                insert_at_end(&mut lines, container.end_line as usize, &block)
            }
        };
        let limit = (container.line as usize).saturating_sub(1);
        let added = add_imports(&mut lines, limit, &imports);
        let text = join_lines(&lines, eol);
        self.validate(&rel, &text)?;
        if dry {
            return Ok(Output {
                lines: block,
                unresolved,
            });
        }
        let start = first + added + 1;
        let end = start + block.len().saturating_sub(1);
        let place = match after {
            Some(method) => format!("{}::{} after {method}", rel, container.name),
            None => format!("{}::{}", rel, container.name),
        };
        let summary = crate::tf!(
            "inserted {} member(s) into {} (L{}-{})",
            "beszúrva {} tag ide: {} (L{}-{})",
            members.len(),
            place,
            start,
            end
        );
        let line = InsertInto { rel, text, summary }.execute(self.env)?;
        Ok(Output {
            lines: vec![line],
            unresolved,
        })
    }

    fn replace(
        &self,
        program: &Program,
        file: &str,
        at: &ReplaceAt,
        expect: Option<&str>,
        dry: bool,
    ) -> GenResult<Output> {
        let rel = self.rel(file)?;
        self.check_extension(&rel)?;
        let Loaded {
            mut lines,
            eol,
            graph,
        } = self.load(&rel)?;
        let (from, to, label) = match at {
            ReplaceAt::Lines(a, b) => {
                if *a == 0 || a > b || *b > lines.len() {
                    return Err(GenError::new(crate::tf!(
                        "bad line range L{}-{} ({} has {} lines)",
                        "hibás sortartomány: L{}-{} ({} {} soros)",
                        a,
                        b,
                        rel,
                        lines.len()
                    )));
                }
                (*a, *b, None)
            }
            ReplaceAt::Method { class, name } => {
                let (start, end) = method_range(&graph, &lines, class.as_deref(), name, &rel)?;
                (start, end, Some(name.as_str()))
            }
        };
        if let Some(prefix) = expect {
            let current =
                crate::update::checksum::sha256_hex(range_text(&lines, from, to).as_bytes());
            let wanted = prefix.trim().to_ascii_lowercase();
            if wanted.len() < 4 || !current.starts_with(&wanted) {
                return Err(GenError::new(crate::tf!(
                    "L{}-{} of {} changed (expected sha256 {}…, current {}…): read it again before replacing",
                    "L{}-{} tartalma megváltozott ebben: {} (várt sha256 {}…, jelenlegi {}…): cserélés előtt olvasd újra",
                    from,
                    to,
                    rel,
                    wanted,
                    &current[..12]
                )));
            }
        }
        let kind = graph
            .containers
            .iter()
            .find(|c| (c.line as usize) <= from && to <= (c.end_line as usize))
            .map_or(TypeKind::Class, |c| TypeKind::from_graph(&c.kind));
        let block: Vec<String> = if program.decls.is_empty() {
            self.emitter.members(kind, &program.loose).collect()
        } else {
            let mut all = Vec::new();
            for (i, decl) in program.decls.iter().enumerate() {
                if i > 0 {
                    all.push(String::new());
                }
                all.extend(self.emitter.declaration(decl));
            }
            all
        };
        let names = self
            .emitter
            .referenced_names(&program.decls, &program.loose);
        let psr4 = Psr4::load(self.env.fs, self.env.root);
        let (imports, unresolved) = self.resolve_names(
            &names,
            program,
            graph.namespace.as_deref(),
            &file_imports(&graph),
            &psr4,
        );
        let tail = lines.split_off(to);
        lines.truncate(from - 1);
        lines.extend(block.iter().cloned());
        lines.extend(tail);
        let added = add_imports(&mut lines, from - 1, &imports);
        let text = join_lines(&lines, eol);
        self.validate(&rel, &text)?;
        if dry {
            return Ok(Output {
                lines: block,
                unresolved,
            });
        }
        let new_from = from + added;
        let new_to = new_from + block.len().saturating_sub(1);
        let name = label.map(|n| format!(" ({n})")).unwrap_or_default();
        let summary = crate::tf!(
            "replaced {} L{}-{} → L{}-{}{}",
            "kicserélve: {} L{}-{} → L{}-{}{}",
            rel,
            from,
            to,
            new_from,
            new_to,
            name
        );
        let line = ReplaceRange { rel, text, summary }.execute(self.env)?;
        Ok(Output {
            lines: vec![line],
            unresolved,
        })
    }
}

/// A `--into` célosztálya: a megnevezett, vagy az egyetlen a fájlban.
fn pick_container<'g>(
    graph: &'g FileGraph,
    class: Option<&str>,
    rel: &str,
) -> GenResult<&'g Container> {
    let short = |name: &str| name.rsplit('\\').next().unwrap_or(name).to_owned();
    let names: Vec<String> = graph.containers.iter().map(|c| short(&c.name)).collect();
    match class {
        Some(wanted) => {
            let wanted = short(wanted);
            graph
                .containers
                .iter()
                .find(|c| short(&c.name) == wanted)
                .ok_or_else(|| {
                    GenError::new(crate::tf!(
                        "no class `{}` in {} (has: {})",
                        "nincs `{}` osztály itt: {} (van: {})",
                        wanted,
                        rel,
                        names.join(", ")
                    ))
                })
        }
        None => match graph.containers.as_slice() {
            [only] => Ok(only),
            [] => Err(GenError::new(crate::tf!(
                "no class / interface / trait / enum in {}",
                "nincs osztály / interfész / trait / enum ebben: {}",
                rel
            ))),
            _ => Err(GenError::new(crate::tf!(
                "{} has several types ({}): name one as {}::Class",
                "{} több típust tartalmaz ({}): nevezd meg így: {}::Osztály",
                rel,
                names.join(", "),
                rel
            ))),
        },
    }
}

/// A konténer metódusai közül a megnevezett utolsó sora (1-alapú).
fn find_method(graph: &FileGraph, container: &Container, name: &str) -> GenResult<usize> {
    let own: Vec<_> = graph
        .methods
        .iter()
        .filter(|m| m.container.as_deref() == Some(container.name.as_str()))
        .collect();
    own.iter()
        .find(|m| m.name == name)
        .map(|m| m.end_line.unwrap_or(m.line) as usize)
        .ok_or_else(|| {
            GenError::new(crate::tf!(
                "no method `{}` in {} (has: {})",
                "nincs `{}` metódus itt: {} (van: {})",
                name,
                container.name,
                own.iter()
                    .map(|m| m.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })
}

/// A metódus teljes sortartománya (docblockkal és attribútumokkal).
fn method_range(
    graph: &FileGraph,
    lines: &[String],
    class: Option<&str>,
    name: &str,
    rel: &str,
) -> GenResult<(usize, usize)> {
    let short = |n: &str| n.rsplit('\\').next().unwrap_or(n).to_owned();
    let matching: Vec<_> = graph
        .methods
        .iter()
        .filter(|m| m.name == name)
        .filter(|m| class.is_none_or(|c| m.container.as_deref().map(short) == Some(short(c))))
        .collect();
    match matching.as_slice() {
        [m] => Ok((
            extend_over_docs(lines, m.line as usize),
            m.end_line.unwrap_or(m.line) as usize,
        )),
        [] => Err(GenError::new(crate::tf!(
            "no method `{}` in {}",
            "nincs `{}` metódus itt: {}",
            name,
            rel
        ))),
        _ => Err(GenError::new(crate::tf!(
            "`{}` exists in several classes of {}: use {}::Class::{}",
            "a(z) `{}` több osztályban is van ebben: {}: használd így: {}::Osztály::{}",
            name,
            rel,
            rel,
            name
        ))),
    }
}
