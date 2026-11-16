//! `rename class <régi> <új>`: egy PHP-osztály átnevezése (és a névtér-váltás) a teljes projekten.
//!
//! - **Deklaráció** + a `namespace` sor (ha a névtér változik) + a PSR-4 fájl-áthelyezés (`app/Models/Fee.php` →
//!   `app/Models/Charge.php`; a névtér-váltásnál a `composer.json` térképe szerinti új mappába).
//! - **Hivatkozás**: a fájl névterével és `use`-aival feloldott név-tokenek (mint a `name_resolver`): `use` sorok
//!   (az alias megmarad), teljesen minősített és rövid nevek, `X::class`, `new X`, statikus hívás, típusjelzés,
//!   és a docblock `@param` / `@return` / `@var` … típusai. A névtér-váltásnál a hiányzó importot pótoljuk.
//! - **Sztring** (`'App\Models\Fee'`): csak jelzés; a `--strings` átírja.
//! - **Eloquent tábla**: a táblanév az osztálynévből következik, ezért `$table` nélkül figyelmeztetünk, és a
//!   `--keep-table` beszúrja a régi nevet.
//! - Vue / JS nem érintett.

use std::collections::BTreeSet;
use std::path::Path;

use super::RenameRequest;
use super::php::{Decl, Kind, PhpFile, Token, find_decl, load_all, name_tokens};
use super::sites::{Plan, Site, Uncertain, line_snippet};
use crate::generate::fs::Env;
use crate::generate::project::Psr4;
use crate::generate::{GenError, GenResult};

/// A régi és az új név részei.
struct Names {
    old_fqcn: String,
    new_fqcn: String,
    old_ns: String,
    new_ns: String,
    old_short: String,
    new_short: String,
}

impl Names {
    fn ns_changed(&self) -> bool {
        self.old_ns != self.new_ns
    }
}

fn split(fqcn: &str) -> (&str, &str) {
    fqcn.rsplit_once('\\').unwrap_or(("", fqcn))
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn names(decl: &Decl, new: &str) -> GenResult<Names> {
    let new = new.trim().trim_start_matches('\\');
    let (old_ns, old_short) = split(&decl.fqcn);
    let new_fqcn = if new.contains('\\') || old_ns.is_empty() {
        new.to_owned()
    } else {
        format!("{old_ns}\\{new}")
    };
    let (new_ns, new_short) = split(&new_fqcn);
    if new_fqcn.is_empty() || !new_fqcn.split('\\').all(is_identifier) {
        return Err(GenError::new(crate::tf!(
            "{} is not a valid class name",
            "a(z) {} nem érvényes osztálynév",
            new
        )));
    }
    if new_fqcn == decl.fqcn {
        return Err(GenError::new(
            crate::t!(
                "the new name equals the old one",
                "az új név megegyezik a régivel"
            )
            .to_owned(),
        ));
    }
    if new_ns.is_empty() != old_ns.is_empty() {
        return Err(GenError::new(
            crate::t!(
                "moving a class into or out of the global namespace is not supported",
                "osztály áthelyezése a globális névtérbe vagy onnan nem támogatott"
            )
            .to_owned(),
        ));
    }
    Ok(Names {
        old_fqcn: decl.fqcn.clone(),
        new_fqcn: new_fqcn.clone(),
        old_ns: old_ns.to_owned(),
        new_ns: new_ns.to_owned(),
        old_short: old_short.to_owned(),
        new_short: new_short.to_owned(),
    })
}

/// A beolvasott fájlok és a kérés közös környezete.
struct Ctx<'a> {
    names: &'a Names,
    old_file: &'a PhpFile,
    old_decl: &'a Decl,
    /// Minden ismert osztály teljes neve, kisbetűvel.
    known: &'a BTreeSet<String>,
    strings: bool,
}

/// A terv az osztály-átnevezéshez: `(a jelentés tárgya, a terv)`.
pub(super) fn plan(env: Env<'_>, req: &RenameRequest) -> GenResult<(String, Plan)> {
    let files = load_all(env.fs, env.root);
    let (old_file, old_decl) = find_decl(&files, &req.old).map_err(GenError::new)?;
    let n = names(old_decl, &req.new)?;
    let taken = files
        .iter()
        .flat_map(|f| f.decls.iter())
        .any(|d| d.fqcn.eq_ignore_ascii_case(&n.new_fqcn));
    if taken {
        return Err(GenError::new(crate::tf!(
            "class {} already exists, refusing the rename (nothing written)",
            "a(z) {} osztály már létezik, az átnevezést elutasítom (nem írtam semmit)",
            n.new_fqcn
        )));
    }
    let known: BTreeSet<String> = files
        .iter()
        .flat_map(|f| f.decls.iter())
        .map(|d| d.fqcn.to_ascii_lowercase())
        .collect();
    let ctx = Ctx {
        names: &n,
        old_file,
        old_decl,
        known: &known,
        strings: req.strings,
    };
    let mut plan = Plan::default();
    declaration(&ctx, &mut plan);
    for file in &files {
        scan_file(&ctx, file, &mut plan)?;
    }
    table(&ctx, req.keep_table, &mut plan);
    relocate(env, &ctx, &mut plan)?;
    Ok((format!("class {} → {}", n.old_fqcn, n.new_fqcn), plan))
}

/// A deklaráció neve és a névtér-sor a deklaráló fájlban.
fn declaration(ctx: &Ctx<'_>, plan: &mut Plan) {
    let (file, decl, n) = (ctx.old_file, ctx.old_decl, ctx.names);
    plan.sites.push(Site::replace(
        &file.rel,
        decl.name_span.0,
        decl.name_span.1,
        &n.new_short,
    ));
    if n.ns_changed()
        && let Some((start, end)) = file.ns_span
    {
        plan.sites
            .push(Site::replace(&file.rel, start, end, &n.new_ns));
    }
}

/// Egy fájl hivatkozásai: importok, név-tokenek, docblockok, sztringek.
fn scan_file(ctx: &Ctx<'_>, file: &PhpFile, plan: &mut Plan) -> GenResult<()> {
    let n = ctx.names;
    let is_old_file = std::ptr::eq(file, ctx.old_file);
    let mut renamed_short = false;
    let mut needed: BTreeSet<String> = BTreeSet::new();

    for import in &file.imports {
        if !import.fqcn.eq_ignore_ascii_case(&n.old_fqcn) {
            continue;
        }
        let (start, end) = import.name_span;
        if !import.group {
            plan.sites
                .push(Site::replace(&file.rel, start, end, &n.new_fqcn));
        } else if n.ns_changed() {
            plan.uncertain.push(Uncertain {
                rel: file.rel.clone(),
                line: file.line_of(start),
                text: format!("group use: {}", line_snippet(&file.text, start)),
            });
        } else {
            let last = file.text[start..end]
                .rfind('\\')
                .map_or(start, |p| start + p + 1);
            plan.sites
                .push(Site::replace(&file.rel, last, end, &n.new_short));
        }
    }

    let mut tokens: Vec<Token> = name_tokens(&file.code, 0);
    for seg in file.segs.iter().filter(|s| s.kind == Kind::Doc) {
        tokens.extend(doc_tokens(&file.text, seg.start, seg.end));
    }
    for token in tokens {
        if file.in_header(token.start) {
            continue;
        }
        let full = file.resolve(&token.text);
        if full.eq_ignore_ascii_case(&n.old_fqcn) {
            if token.text.contains('\\') {
                if n.ns_changed() {
                    plan.sites.push(Site::replace(
                        &file.rel,
                        token.start,
                        token.end,
                        format!("\\{}", n.new_fqcn),
                    ));
                } else {
                    let last = token.start + token.text.rfind('\\').map_or(0, |p| p + 1);
                    plan.sites
                        .push(Site::replace(&file.rel, last, token.end, &n.new_short));
                }
                continue;
            }
            let import = file
                .imports
                .iter()
                .find(|i| i.alias.eq_ignore_ascii_case(&token.text));
            match import {
                Some(i) if i.explicit_alias || (i.group && n.ns_changed()) => {}
                Some(_) => {
                    plan.sites.push(Site::replace(
                        &file.rel,
                        token.start,
                        token.end,
                        &n.new_short,
                    ));
                    renamed_short = true;
                }
                None => {
                    plan.sites.push(Site::replace(
                        &file.rel,
                        token.start,
                        token.end,
                        &n.new_short,
                    ));
                    renamed_short = true;
                    if n.ns_changed() && !is_old_file {
                        needed.insert(n.new_fqcn.clone());
                    }
                }
            }
        } else if n.ns_changed()
            && is_old_file
            && !n.old_ns.is_empty()
            && !token.text.contains('\\')
            && !file
                .imports
                .iter()
                .any(|i| i.alias.eq_ignore_ascii_case(&token.text))
            && !file
                .decls
                .iter()
                .any(|d| d.name.eq_ignore_ascii_case(&token.text))
        {
            // a névtér-váltás után a régi névtér testvér-osztályai már nem érhetők el rövid néven
            let sibling = format!("{}\\{}", n.old_ns, token.text);
            if ctx.known.contains(&sibling.to_ascii_lowercase()) {
                needed.insert(sibling);
            }
        }
    }

    if renamed_short && !is_old_file {
        let clash = file.imports.iter().any(|i| {
            i.alias.eq_ignore_ascii_case(&n.new_short) && !i.fqcn.eq_ignore_ascii_case(&n.old_fqcn)
        }) || file
            .decls
            .iter()
            .any(|d| d.name.eq_ignore_ascii_case(&n.new_short));
        if clash {
            return Err(GenError::new(crate::tf!(
                "{} already uses the name {} — refusing the rename (nothing written)",
                "{} már használja a(z) {} nevet — az átnevezést elutasítom (nem írtam semmit)",
                file.rel,
                n.new_short
            )));
        }
    }
    if !needed.is_empty() {
        let eol = file.eol();
        let lines: String = needed
            .iter()
            .map(|fqcn| format!("use {fqcn};{eol}"))
            .collect();
        // import nélküli fájlban üres sor választja el a névtér-sortól
        let text = if file.imports.is_empty() {
            format!("{eol}{lines}")
        } else {
            lines
        };
        plan.sites
            .push(Site::insert(&file.rel, file.insert_at, text));
    }
    strings(ctx, file, plan);
    Ok(())
}

/// A docblock típus-tokenei: a `@param` / `@return` / `@var` … utáni első típus-kifejezés.
fn doc_tokens(text: &str, start: usize, end: usize) -> Vec<Token> {
    const TAGS: [&str; 14] = [
        "@param",
        "@return",
        "@var",
        "@throws",
        "@property",
        "@property-read",
        "@property-write",
        "@method",
        "@mixin",
        "@see",
        "@extends",
        "@implements",
        "@use",
        "@template-extends",
    ];
    let mut out = Vec::new();
    let mut line_start = start;
    for line in text[start..end].split_inclusive('\n') {
        let trimmed = line.trim_start();
        let trimmed = trimmed.strip_prefix("/**").unwrap_or(trimmed).trim_start();
        let trimmed = trimmed.trim_start_matches('*').trim_start();
        let lead = line.len() - trimmed.len();
        let tag = trimmed.split_whitespace().next().unwrap_or("");
        if TAGS.contains(&tag) {
            let after = &trimmed[tag.len()..];
            let skip = after.len() - after.trim_start().len();
            let expr = &after[skip..];
            // a típus-kifejezés vége: az első szóköz a `<>` / `()` / `{}` zárójelein kívül
            let mut depth = 0i32;
            let mut stop = expr.len();
            for (i, c) in expr.char_indices() {
                match c {
                    '<' | '(' | '{' | '[' => depth += 1,
                    '>' | ')' | '}' | ']' => depth -= 1,
                    ws if ws.is_whitespace() && depth <= 0 => {
                        stop = i;
                        break;
                    }
                    _ => {}
                }
            }
            let base = line_start + lead + tag.len() + skip;
            out.extend(name_tokens(&expr[..stop], base));
        }
        line_start += line.len();
    }
    out
}

/// A sztring-hivatkozások: jelzés, vagy `--strings`-szel átírás (az escape-stílus megmarad).
fn strings(ctx: &Ctx<'_>, file: &PhpFile, plan: &mut Plan) {
    let n = ctx.names;
    if !n.old_fqcn.contains('\\') {
        return;
    }
    let styles = [
        (
            n.old_fqcn.replace('\\', "\\\\"),
            n.new_fqcn.replace('\\', "\\\\"),
        ),
        (n.old_fqcn.clone(), n.new_fqcn.clone()),
    ];
    for seg in file.segs.iter().filter(|s| s.kind == Kind::Str) {
        let content = &file.text[seg.start..seg.end];
        for (needle, replacement) in &styles {
            for (at, _) in content.match_indices(needle.as_str()) {
                if !bounded(content, at, needle.len()) {
                    continue;
                }
                let start = seg.start + at;
                if ctx.strings {
                    plan.sites.push(Site::replace(
                        &file.rel,
                        start,
                        start + needle.len(),
                        replacement.as_str(),
                    ));
                } else {
                    plan.uncertain.push(Uncertain {
                        rel: file.rel.clone(),
                        line: file.line_of(start),
                        text: line_snippet(&file.text, start),
                    });
                }
            }
        }
    }
}

/// A találat egész név-e (nem egy hosszabb névtér része).
fn bounded(s: &str, at: usize, len: usize) -> bool {
    let b = s.as_bytes();
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut p = at;
    while p > 0 && b[p - 1] == b'\\' {
        p -= 1;
    }
    let lead_ok = (at - p) <= 2 && (p == 0 || !ident(b[p - 1]));
    let tail_ok = at + len >= b.len() || !ident(b[at + len]);
    lead_ok && tail_ok
}

/// Az Eloquent táblanév az osztálynévből: snake_case, többes szám (`Fee` → `fees`).
pub(super) fn table_name(class: &str) -> String {
    let mut snake = String::new();
    for (i, c) in class.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                snake.push('_');
            }
            snake.push(c.to_ascii_lowercase());
        } else {
            snake.push(c);
        }
    }
    let vowel = |c: char| "aeiou".contains(c);
    if let Some(stem) = snake.strip_suffix('y')
        && stem.chars().last().is_some_and(|c| !vowel(c))
    {
        return format!("{stem}ies");
    }
    if ["s", "x", "z", "ch", "sh"]
        .iter()
        .any(|e| snake.ends_with(e))
    {
        return format!("{snake}es");
    }
    format!("{snake}s")
}

/// Az Eloquent-modell `$table` nélkül: a táblanév változna.
fn table(ctx: &Ctx<'_>, keep: bool, plan: &mut Plan) {
    let (file, decl, n) = (ctx.old_file, ctx.old_decl, ctx.names);
    if decl.kind != "class" || n.old_short == n.new_short {
        return;
    }
    let base = decl.extends.as_deref().unwrap_or("");
    let model = base.rsplit('\\').next().unwrap_or("").ends_with("Model")
        || (!base.is_empty() && file.text.contains("Illuminate\\Database\\Eloquent"));
    let body = &file.code[decl.open..decl.close];
    if !model || body.contains("$table") {
        return;
    }
    let (old_table, new_table) = (table_name(&n.old_short), table_name(&n.new_short));
    if old_table == new_table {
        return;
    }
    if keep {
        let eol = file.eol();
        let after = decl.open + 1;
        let rest = &file.text[after..];
        let line = format!("    protected $table = '{old_table}';");
        if rest.starts_with("\r\n") || rest.starts_with('\n') {
            let skip = if rest.starts_with("\r\n") { 2 } else { 1 };
            plan.sites.push(Site::insert(
                &file.rel,
                after + skip,
                format!("{line}{eol}{eol}"),
            ));
        } else {
            plan.sites
                .push(Site::insert(&file.rel, after, format!("{eol}{line}")));
        }
        plan.notes.push(crate::tf!(
            "table name kept: protected $table = '{}' added",
            "a táblanév megmarad: protected $table = '{}' beszúrva",
            old_table
        ));
    } else {
        plan.notes.push(crate::tf!(
            "⚠ table name changes {} → {} (add protected $table = '{}' or a migration; --keep-table adds it)",
            "⚠ a táblanév változik: {} → {} (add hozzá: protected $table = '{}', vagy írj migrációt; a --keep-table beszúrja)",
            old_table,
            new_table,
            old_table
        ));
    }
}

/// A PSR-4 fájl-áthelyezés.
fn relocate(env: Env<'_>, ctx: &Ctx<'_>, plan: &mut Plan) -> GenResult<()> {
    let (file, n) = (ctx.old_file, ctx.names);
    let stem = Path::new(&file.rel)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if file.decls.len() != 1 || stem != n.old_short {
        plan.notes.push(crate::tf!(
            "⚠ {} not moved (it does not hold exactly this class by PSR-4 name)",
            "⚠ a(z) {} nem lett áthelyezve (nem pontosan ez az osztály van benne PSR-4 névvel)",
            file.rel
        ));
        return Ok(());
    }
    let target =
        if n.ns_changed() {
            Psr4::load(env.fs, env.root).rel_of(&n.new_fqcn).ok_or_else(|| {
            GenError::new(crate::tf!(
                "cannot place {} by composer.json psr-4 (nothing written)",
                "a(z) {} nem helyezhető el a composer.json psr-4 szerint (nem írtam semmit)",
                n.new_fqcn
            ))
        })?
        } else {
            let dir = file.rel.rsplit_once('/').map_or("", |(d, _)| d);
            if dir.is_empty() {
                format!("{}.php", n.new_short)
            } else {
                format!("{dir}/{}.php", n.new_short)
            }
        };
    if target != file.rel {
        plan.moves.push((file.rel.clone(), target));
    }
    Ok(())
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn table_names_follow_the_eloquent_convention() {
        assert_eq!(table_name("Fee"), "fees");
        assert_eq!(table_name("Charge"), "charges");
        assert_eq!(table_name("BlogPost"), "blog_posts");
        assert_eq!(table_name("Category"), "categories");
        assert_eq!(table_name("Address"), "addresses");
    }

    #[test]
    fn bounded_rejects_longer_namespaces() {
        let s = "Vendor\\App\\Fee and App\\Fee";
        let first = s.find("App\\Fee").unwrap();
        let last = s.rfind("App\\Fee").unwrap();
        assert!(!bounded(s, first, 7));
        assert!(bounded(s, last, 7));
    }
}
