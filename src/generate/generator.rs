//! Facade: a `Generator` az egyetlen belépő a CLI-nek és az MCP-nek. Összefogja a lépéseket: kifejezés → AST
//! (Interpreter) → projekt-ismeret (névtér, stílus, importok a feloldó-láncból) → emitter (Strategy) → fájl
//! (Builder) → ellenőrzés (a nyelv parserével) → parancs (Command: írás mentéssel). Semmi nem íródik, amíg a teljes
//! új szöveg nem parse-ol hibátlanul.

use std::collections::BTreeSet;

use super::ast::{Member, Program, RawBlock, TypeKind};
use super::command::{EditCommand, InsertInto, ReplaceRange, Undo, WriteNew};
use super::edit::{
    add_imports, extend_over_docs, insert_after_line, insert_at_end, insert_before_line, join_lines,
};
use super::emitter::{Emitter, FileSpec};
use super::error::{GenError, GenResult, Kind};
use super::fs::Env;
use super::parser;
use super::project::{Psr4, detect_strict_types};
use super::report;
use super::resolve::{PhpScan, ResolverChain};
use super::target;
use crate::model::{Container, FileGraph};

/// Hová menjen a generált kód.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Nincs célfájl: a generált kód a kimenet (a `--dry` is ide tartozik).
    Preview,
    /// Új fájl (létezőt nem ír felül).
    Write { file: String },
    /// Tagok beszúrása egy osztályba (vagy egy Vue-sablon elemébe: `class` = `template#id`).
    Into {
        file: String,
        class: Option<String>,
        /// Hová az osztályon belül.
        place: Place,
        /// `--upsert`: a már létező tagot lecseréli (egy csupasz taggal).
        upsert: bool,
        /// `--expect` az `--upsert`-tel: csak akkor ellenőrzi, ha a tag már létezik (csere); beszúrásnál nincs mit
        /// ellenőrizni, ezért ott figyelmen kívül marad.
        expect: Option<String>,
    },
    /// Egy tartomány / metódus cseréje.
    Replace {
        file: String,
        at: ReplaceAt,
        expect: Option<String>,
    },
}

/// Hová szúr be az `--into` az osztályon belül.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Place {
    /// Az osztály végére (alapértelmezés).
    #[default]
    End,
    /// `--after <metódus>`: ennek a metódusnak a végére.
    After(String),
    /// `--before <metódus>`: ennek a metódusnak (a docblockjának) az elé.
    Before(String),
    /// `--at L<n>`: az n. (1-alapú) sor elé (a célosztály törzsében).
    Line(usize),
}

impl Place {
    /// A CLI három, egymást kizáró kapcsolójából (`--after`, `--before`, `--at`) a hely.
    pub fn of(after: Option<String>, before: Option<String>, at: Option<usize>) -> Self {
        match (after, before, at) {
            (Some(m), _, _) => Self::After(m),
            (None, Some(m), _) => Self::Before(m),
            (None, None, Some(n)) => Self::Line(n),
            (None, None, None) => Self::End,
        }
    }

    /// Az `--after` neve (a Vue / HTML célok csak ezt ismerik: `L<n>`, tagnév vagy elem); a többi hely ott nem számít.
    pub fn after_name(&self) -> Option<&str> {
        match self {
            Self::After(m) => Some(m.as_str()),
            _ => None,
        }
    }
}

/// Mit cseréljen a `Target::Replace`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplaceAt {
    /// Sortartomány (1-alapú, zárt).
    Lines(usize, usize),
    /// Egy metódus (a docblockjával és az attribútumaival); `class`: ha több osztályban is van ilyen.
    Method { class: Option<String>, name: String },
}

/// Mi történjen a kimenettel: kiírás a fájlba, vagy előnézet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    /// Kiírás (mentéssel, ellenőrzés után).
    #[default]
    Write,
    /// `--dry`: semmi nem íródik, a kimenet a generált kód.
    Dry,
    /// `--diff`: mint a `Dry`, de a `--replace` a régi / új különbséget mutatja.
    Diff,
}

impl Mode {
    /// Előnézet-e (semmi nem íródik): a `Diff` is az.
    pub fn dry(self) -> bool {
        !matches!(self, Self::Write)
    }

    /// A `--replace` előnézete a különbség legyen-e.
    pub fn diff(self) -> bool {
        self == Self::Diff
    }

    /// A CLI `--dry` / `--diff` kapcsolóiból (a `--diff` erősebb).
    pub fn of(dry: bool, diff: bool) -> Self {
        if diff {
            Self::Diff
        } else if dry {
            Self::Dry
        } else {
            Self::Write
        }
    }
}

/// Egy generálási kérés.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenRequest {
    pub expr: String,
    pub mode: Mode,
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

/// Az `undo` a generátor / átnevezés mentéseiből: a fájl (vagy a legutóbb mentett fájl / csoport) visszaállítása.
/// Az emitterhez nincs köze, ezért önálló függvény (a CLI és az MCP is ezt hívja).
pub fn undo(env: Env<'_>, file: Option<&str>) -> GenResult<Output> {
    let rel = file.map(|f| target::rel(env.root, f)).transpose()?;
    let line = Undo { rel }.execute(env)?;
    Ok(Output {
        lines: vec![line],
        unresolved: Vec::new(),
    })
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
        if let Some(raw) = &program.raw {
            return self.run_raw(&program, raw, request);
        }
        match &request.target {
            Target::Preview => self.preview(&program),
            Target::Write { file } => self.write(&program, file, request.mode),
            Target::Into {
                file,
                class,
                place,
                upsert,
                expect,
            } => self.insert_members(
                &program,
                file,
                class.as_deref(),
                place,
                *upsert,
                expect.as_deref(),
                request.mode,
            ),
            Target::Replace { file, at, expect } => {
                self.replace(&program, file, at, expect.as_deref(), request.mode)
            }
        }
    }

    /// A nyers blokk (`php{…}` / `raw{…}`) kérése: előnézet, új fájl, beszúrás (`--at` / `--after L<n>` / a fájl vége)
    /// vagy sortartomány cseréje.
    fn run_raw(
        &self,
        program: &Program,
        raw: &RawBlock,
        request: &GenRequest,
    ) -> GenResult<Output> {
        let block = raw_lines(&raw.text);
        // üres blokk csak `--replace`-nél van értelme: a tartomány törlése
        if block.is_empty() && !matches!(request.target, Target::Replace { .. }) {
            return Err(GenError::new(
                crate::t!(
                    "the raw block is empty (an empty `php{}` only deletes a range with --replace)",
                    "a nyers blokk üres (az üres `php{}` csak a --replace-szel töröl egy tartományt)"
                )
                .to_owned(),
            ));
        }
        let psr4 = Psr4::load(self.env.fs, self.env.root);
        let scan = PhpScan::new(self.env.fs, self.env.root, &psr4);
        let names = if raw.php {
            self.emitter.raw_names(&raw.text)
        } else {
            BTreeSet::new()
        };
        match &request.target {
            Target::Preview => {
                let (_, unresolved) = self.resolve_names(&names, program, None, &[], &psr4, &scan);
                Ok(Output {
                    lines: block,
                    unresolved,
                })
            }
            Target::Write { file } => {
                let rel = self.rel(file)?;
                self.check_extension(&rel)?;
                let (imports, unresolved) =
                    self.resolve_names(&names, program, None, &[], &psr4, &scan);
                let mut lines: Vec<String> = Vec::new();
                if raw.php {
                    lines.push("<?php".to_owned());
                    lines.push(String::new());
                }
                lines.extend(block.iter().cloned());
                lines.push(String::new());
                let limit = lines.len();
                add_imports(&mut lines, limit, &imports);
                let text = join_lines(&lines, "\n");
                self.validate(&rel, &text)?;
                if request.mode.dry() {
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
            Target::Into { file, place, .. } => {
                let rel = self.rel(file)?;
                self.check_extension(&rel)?;
                let Loaded {
                    mut lines,
                    eol,
                    graph,
                } = self.load(&rel)?;
                let (imports, unresolved) = self.resolve_names(
                    &names,
                    program,
                    graph.namespace.as_deref(),
                    &file_imports(&graph),
                    &psr4,
                    &scan,
                );
                // egy sorhoz kötött beszúrásnál a blokk az ottani behúzást kapja (egy záráson belüli utasítás)
                let anchor = match place {
                    &Place::Line(n) => Some(n),
                    Place::After(at) => at.strip_prefix('L').and_then(|d| d.parse::<usize>().ok()),
                    _ => None,
                };
                let block = indented(
                    &block,
                    anchor.and_then(|n| lines.get(n.wrapping_sub(1))),
                    raw.php,
                );
                let first = match place {
                    Place::End => {
                        let last = lines
                            .iter()
                            .rposition(|l| !l.trim().is_empty())
                            .map_or(0, |i| i + 1);
                        insert_after_line(&mut lines, last, &block)
                    }
                    &Place::Line(n) => {
                        check_line(n, lines.len(), &rel)?;
                        insert_before_line(&mut lines, n, &block)
                    }
                    Place::After(at) => {
                        let n = at
                            .strip_prefix('L')
                            .and_then(|d| d.parse::<usize>().ok())
                            .ok_or_else(|| {
                                GenError::new(crate::t!(
                                    "a raw block goes after a line: `--after L<n>` (or `--at L<n>`, or nothing for the end of the file)",
                                    "a nyers blokk egy sor után mehet: `--after L<n>` (vagy `--at L<n>`, vagy semmi: a fájl végére)"
                                ))
                            })?;
                        check_line(n, lines.len(), &rel)?;
                        insert_after_line(&mut lines, n, &block)
                    }
                    Place::Before(_) => {
                        return Err(GenError::new(
                            crate::t!(
                                "a raw block cannot go `--before` a method: use `--at L<n>`",
                                "a nyers blokk nem mehet metódus `--before` elé: használd az `--at L<n>`-t"
                            )
                            .to_owned(),
                        ));
                    }
                };
                let added = add_imports(&mut lines, first, &imports);
                let text = join_lines(&lines, eol);
                self.validate(&rel, &text)?;
                let start = first + added + 1;
                let end = start + block.len() - 1;
                if request.mode.dry() {
                    return Ok(Output {
                        lines: report::with_header(report::insert_header(&rel, start), block),
                        unresolved,
                    });
                }
                let summary = report::inserted_lines(block.len(), &rel, start, end);
                let line = InsertInto { rel, text, summary }.execute(self.env)?;
                Ok(Output {
                    lines: vec![line],
                    unresolved,
                })
            }
            Target::Replace { file, at, expect } => {
                let rel = self.rel(file)?;
                self.check_extension(&rel)?;
                let Loaded {
                    mut lines,
                    eol,
                    graph,
                } = self.load(&rel)?;
                let (from, to) = match at {
                    ReplaceAt::Lines(a, b) => (*a, *b),
                    ReplaceAt::Method { class, name } => {
                        method_range(&graph, &lines, class.as_deref(), name, &rel)?
                    }
                };
                if from == 0 || from > to || to > lines.len() {
                    return Err(GenError::new(crate::tf!(
                        "bad line range L{}-{} ({} has {} lines)",
                        "hibás sortartomány: L{}-{} ({} {} soros)",
                        from,
                        to,
                        rel,
                        lines.len()
                    )));
                }
                target::check_expect(&lines, &rel, from, to, expect.as_deref())?;
                let block = indented(&block, lines.get(from - 1), raw.php);
                let (imports, unresolved) = self.resolve_names(
                    &names,
                    program,
                    graph.namespace.as_deref(),
                    &file_imports(&graph),
                    &psr4,
                    &scan,
                );
                let old: Vec<String> = lines[from - 1..to].to_vec();
                let tail = lines.split_off(to);
                lines.truncate(from - 1);
                lines.extend(block.iter().cloned());
                lines.extend(tail);
                let added = add_imports(&mut lines, from - 1, &imports);
                let text = join_lines(&lines, eol);
                self.validate(&rel, &text)?;
                let new_from = from + added;
                let new_to = new_from + block.len().saturating_sub(1);
                if request.mode.dry() {
                    let header = report::replace_header(&rel, (from, to), (new_from, new_to), "");
                    return Ok(Output {
                        lines: report::replace_preview(header, &old, block, request.mode.diff()),
                        unresolved,
                    });
                }
                let summary = report::replaced(&rel, (from, to), (new_from, new_to), "");
                let line = ReplaceRange { rel, text, summary }.execute(self.env)?;
                Ok(Output {
                    lines: vec![line],
                    unresolved,
                })
            }
        }
    }

    /// Az `undo`: a fájl (vagy a legutóbb mentett fájl) legutóbbi mentésének visszaállítása.
    pub fn undo(&self, file: Option<&str>) -> GenResult<Output> {
        undo(self.env, file)
    }

    /// A felhasználói fájl-út projekt-relatív, `/`-es alakja (lásd `target::rel`).
    fn rel(&self, file: &str) -> GenResult<String> {
        target::rel(self.env.root, file)
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
        if rel.ends_with(".blade.php") {
            // a Blade-nézet nem tiszta PHP: a PHP-parser nem ellenőrizheti
            return Ok(());
        }
        self.emitter.check(rel, text).map_err(|(line, message)| {
            GenError::new(crate::tf!(
                "syntax error at L{}: {} (nothing written)",
                "szintaktikai hiba az L{}. sorban: {} (nem írtam semmit)",
                line,
                message
            ))
            .with_kind(Kind::Syntax)
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
        scan: &PhpScan<'_>,
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
        let chain = ResolverChain::php(explicit.clone(), scan, psr4, psr4.is_laravel());
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
        let scan = PhpScan::new(self.env.fs, self.env.root, &psr4);
        let (imports, unresolved) =
            self.resolve_names(&names, program, namespace.as_deref(), &[], &psr4, &scan);
        let strict_types = detect_strict_types(&scan);
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
        let scan = PhpScan::new(self.env.fs, self.env.root, &psr4);
        let (_, unresolved) = self.resolve_names(&names, program, None, &[], &psr4, &scan);
        Ok(Output {
            lines: block,
            unresolved,
        })
    }

    fn write(&self, program: &Program, file: &str, mode: Mode) -> GenResult<Output> {
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

    /// A meglévő fájl beolvasása és elemzése.
    fn load(&self, rel: &str) -> GenResult<Loaded> {
        let source = target::load(self.env, rel)?;
        let graph = self.emitter.analyze(rel, &source.text);
        Ok(Loaded {
            lines: source.lines,
            eol: source.eol,
            graph,
        })
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

    #[allow(clippy::too_many_arguments)]
    fn insert_members(
        &self,
        program: &Program,
        file: &str,
        class: Option<&str>,
        place: &Place,
        upsert: bool,
        expect: Option<&str>,
        mode: Mode,
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
        if let Some(at) = upsert_target(upsert, program, &graph, container) {
            return self.replace(program, file, &at, expect, mode);
        }
        let kind = TypeKind::from_graph(&container.kind);
        let block: Vec<String> = self.emitter.members(kind, members).collect();
        let names = self.emitter.referenced_names(&[], members);
        let psr4 = Psr4::load(self.env.fs, self.env.root);
        let scan = PhpScan::new(self.env.fs, self.env.root, &psr4);
        let (imports, unresolved) = self.resolve_names(
            &names,
            program,
            graph.namespace.as_deref(),
            &file_imports(&graph),
            &psr4,
            &scan,
        );
        let first = match place {
            Place::After(method) => {
                let (_, end) = find_method(&graph, container, method)?;
                insert_after_line(&mut lines, end, &block)
            }
            Place::Before(method) => {
                let (start, _) = find_method(&graph, container, method)?;
                let start = extend_over_docs(&lines, start);
                insert_before_line(&mut lines, start, &block)
            }
            &Place::Line(n) => {
                // a törzs az osztály nyitó `{` sorát követő sorral kezdődik (Allman-stílusnál a `{` külön sorban van)
                let open = body_open_line(&lines, container);
                if n <= open || n > container.end_line as usize {
                    return Err(GenError::new(crate::tf!(
                        "--at L{} is not inside the body of {} (L{}-L{})",
                        "az --at L{} nincs a(z) {} törzsében (L{}-L{})",
                        n,
                        container.name,
                        open + 1,
                        container.end_line
                    )));
                }
                insert_before_line(&mut lines, n, &block)
            }
            Place::End => {
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
        let start = first + added + 1;
        let end = start + block.len().saturating_sub(1);
        let label = match place {
            Place::After(method) => format!("{}::{} after {method}", rel, container.name),
            Place::Before(method) => format!("{}::{} before {method}", rel, container.name),
            &Place::Line(n) => format!("{}::{} at L{n}", rel, container.name),
            Place::End => format!("{}::{}", rel, container.name),
        };
        if mode.dry() {
            return Ok(Output {
                lines: report::with_header(report::insert_header(&label, start), block),
                unresolved,
            });
        }
        let summary = report::inserted_members(members.len(), &label, start, end);
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
        mode: Mode,
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
        target::check_expect(&lines, &rel, from, to, expect)?;
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
        let scan = PhpScan::new(self.env.fs, self.env.root, &psr4);
        let (imports, unresolved) = self.resolve_names(
            &names,
            program,
            graph.namespace.as_deref(),
            &file_imports(&graph),
            &psr4,
            &scan,
        );
        let old: Vec<String> = lines[from - 1..to].to_vec();
        let tail = lines.split_off(to);
        lines.truncate(from - 1);
        lines.extend(block.iter().cloned());
        lines.extend(tail);
        let added = add_imports(&mut lines, from - 1, &imports);
        let text = join_lines(&lines, eol);
        self.validate(&rel, &text)?;
        let new_from = from + added;
        let new_to = new_from + block.len().saturating_sub(1);
        let label = label.unwrap_or_default();
        if mode.dry() {
            let header = report::replace_header(&rel, (from, to), (new_from, new_to), label);
            return Ok(Output {
                lines: report::replace_preview(header, &old, block, mode.diff()),
                unresolved,
            });
        }
        let summary = report::replaced(&rel, (from, to), (new_from, new_to), label);
        let line = ReplaceRange { rel, text, summary }.execute(self.env)?;
        Ok(Output {
            lines: vec![line],
            unresolved,
        })
    }
}

/// A `--into` célosztálya: a megnevezett, vagy az egyetlen a fájlban.
pub(super) fn pick_container<'g>(
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

/// `--upsert`: ha egyetlen csupasz tagot kell beszúrni, és a konténernek már van ilyen nevű metódusa, a csere célja.
fn upsert_target(
    upsert: bool,
    program: &Program,
    graph: &FileGraph,
    container: &Container,
) -> Option<ReplaceAt> {
    if !upsert || !program.decls.is_empty() {
        return None;
    }
    let [member] = program.loose.as_slice() else {
        return None;
    };
    let exists = graph
        .methods
        .iter()
        .any(|m| m.name == member.name && m.container.as_deref() == Some(container.name.as_str()));
    exists.then(|| ReplaceAt::Method {
        class: Some(container.name.clone()),
        name: member.name.clone(),
    })
}

/// A konténer nyitó `{` jelét tartalmazó sor (1-alapú); ha nem található, a deklaráció sora.
fn body_open_line(lines: &[String], container: &Container) -> usize {
    let decl = container.line as usize;
    let end = (container.end_line as usize).min(lines.len());
    (decl..=end)
        .find(|&i| i >= 1 && lines[i - 1].contains('{'))
        .unwrap_or(decl)
}

/// A konténer megnevezett metódusának első és utolsó sora (1-alapú).
fn find_method(graph: &FileGraph, container: &Container, name: &str) -> GenResult<(usize, usize)> {
    let own: Vec<_> = graph
        .methods
        .iter()
        .filter(|m| m.container.as_deref() == Some(container.name.as_str()))
        .collect();
    own.iter()
        .find(|m| m.name == name)
        .map(|m| (m.line as usize, m.end_line.unwrap_or(m.line) as usize))
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

/// A nyers blokk sorai: a `{`-t követő első sor balról vágva, a többi a közös behúzással balra tolva; a blokk eleji és
/// végi üres sorok nélkül.
fn raw_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<&str> = text.lines().collect();
    let first_blank = lines.first().is_none_or(|l| l.trim().is_empty());
    while lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return Vec::new();
    }
    let skip = usize::from(!first_blank);
    let indent = lines
        .iter()
        .skip(skip)
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            if i < skip {
                l.trim().to_owned()
            } else if l.trim().is_empty() {
                String::new()
            } else {
                l[indent.min(l.len() - l.trim_start().len())..]
                    .trim_end()
                    .to_owned()
            }
        })
        .collect()
}

/// A PHP-blokk sorai a `like` sor behúzásával (nem PHP-nál vagy ha nincs sor: változatlanul).
fn indented(block: &[String], like: Option<&String>, php: bool) -> Vec<String> {
    let pad: String = like
        .filter(|_| php)
        .map(|l| l[..l.len() - l.trim_start().len()].to_owned())
        .unwrap_or_default();
    block
        .iter()
        .map(|l| {
            if l.is_empty() {
                String::new()
            } else {
                format!("{pad}{l}")
            }
        })
        .collect()
}

/// A sorszám (1-alapú) a fájlon belül van-e.
fn check_line(n: usize, len: usize, rel: &str) -> GenResult<()> {
    if n == 0 || n > len {
        return Err(GenError::new(crate::tf!(
            "line L{} is outside {} ({} lines)",
            "az L{}. sor kívül esik ezen: {} ({} soros)",
            n,
            rel,
            len
        )));
    }
    Ok(())
}
