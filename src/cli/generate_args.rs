//! A `gen` és az `undo` parancs kapcsolói:
//! `callgraph gen '<kifejezés>' [--dry] [--diff] [--write <fájl> | --into <fájl>[::Osztály | ::template#id]
//! [--after <metódus> | --before <metódus> | --at L<n>] [--upsert] |
//! --replace <fájl>(:L<a>-L<b> | ::<metódus>) [--expect <sha256-előtag>]] [--project alias|mappa]`,
//! `callgraph undo [<fájl>] [--project alias|mappa]`. Külön fájlban, hogy az `args.rs` csak egy-egy sorral bővüljön.

use crate::error::{CallgraphError, Result};
use crate::generate::{GenRequest, Mode, Place, ReplaceAt, Target};

use super::CliArgs;

fn gen_usage() -> String {
    crate::t!(
        "callgraph gen '<expr>' [--dry|--diff] [--write <file> | --into <file>[::Class] [--after <method> | --before <method> | --at L<n>] [--upsert] | --replace <file>(:L<a>-L<b> | ::<method>) [--expect <sha256-prefix>]] [--project alias]  (e.g. gen 'c!:Pay<Base> > ctor(-Client $http) + +charge(Order $o):Receipt' --write app/Pay.php)",
        "callgraph gen '<kifejezés>' [--dry|--diff] [--write <fájl> | --into <fájl>[::Osztály] [--after <metódus> | --before <metódus> | --at L<n>] [--upsert] | --replace <fájl>(:L<a>-L<b> | ::<metódus>) [--expect <sha256-előtag>]] [--project alias]  (pl. gen 'c!:Pay<Base> > ctor(-Client $http) + +charge(Order $o):Receipt' --write app/Pay.php)"
    )
    .to_owned()
}

fn undo_usage() -> String {
    crate::t!(
        "callgraph undo [<file>] [--project alias]  (restores the newest backup from .callgraph/backups/)",
        "callgraph undo [<fájl>] [--project alias]  (visszaállítja a legújabb mentést a .callgraph/backups/ mappából)"
    )
    .to_owned()
}

/// A használati hiba szövege az MCP-hez: a záró `gen` / `undo` használati sor nélkül (a séma ismert, a sor ~60 token
/// minden hibánál). A CLI stderr formátuma változatlan; ha a hiba csak maga a használati sor, az marad.
pub(crate) fn compact_usage(message: &str) -> String {
    for usage in [gen_usage(), undo_usage()] {
        if let Some(head) = message.strip_suffix(usage.as_str()) {
            let head = head.trim_end();
            if !head.is_empty() {
                return head.to_owned();
            }
        }
    }
    message.to_owned()
}

fn usage_error(usage: String) -> CallgraphError {
    CallgraphError::Usage(usage)
}

/// A kapcsoló értéke (a következő argumentum).
fn value(it: &mut std::slice::Iter<'_, String>, usage: &dyn Fn() -> String) -> Result<String> {
    it.next().cloned().ok_or_else(|| usage_error(usage()))
}

/// `--replace <fájl>:L42-L87` / `<fájl>:42-87` / `<fájl>::metódus` / `<fájl>::Osztály::metódus`.
fn parse_replace(spec: &str) -> Result<(String, ReplaceAt)> {
    if let Some((file, rest)) = spec.split_once("::") {
        let (class, name) = match rest.rsplit_once("::") {
            Some((class, name)) => (Some(class.to_owned()), name),
            None => (None, rest),
        };
        if file.is_empty() || name.is_empty() {
            return Err(usage_error(gen_usage()));
        }
        return Ok((
            file.to_owned(),
            ReplaceAt::Method {
                class,
                name: name.to_owned(),
            },
        ));
    }
    let range = spec.rsplit_once(':').and_then(|(file, range)| {
        let cleaned = range.replace('L', "");
        let (from, to) = cleaned.split_once('-')?;
        Some((
            file.to_owned(),
            from.trim().parse::<usize>().ok()?,
            to.trim().parse::<usize>().ok()?,
        ))
    });
    match range {
        Some((file, from, to)) if !file.is_empty() => Ok((file, ReplaceAt::Lines(from, to))),
        _ => Err(CallgraphError::Usage(crate::tf!(
            "Bad --replace target: {spec} (expected <file>::<method> or <file>:L42-L87). {}",
            "Hibás --replace cél: {spec} (<fájl>::<metódus> vagy <fájl>:L42-L87 a jó alak). {}",
            gen_usage()
        ))),
    }
}

/// `--at L42` / `--at 42`: pozitív sorszám.
fn parse_line(text: &str) -> Result<usize> {
    text.trim()
        .trim_start_matches(['L', 'l'])
        .parse::<usize>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| {
            CallgraphError::Usage(crate::tf!(
                "Bad --at line: {text} (expected L<n>). {}",
                "Hibás --at sor: {text} (L<n> a jó alak). {}",
                gen_usage()
            ))
        })
}

/// `gen '<kifejezés>' [kapcsolók]`.
pub(crate) fn parse_gen(args: &[String]) -> Result<CliArgs> {
    let (mut dry, mut project) = (false, None);
    let (mut write, mut into, mut after, mut replace, mut expect) = (None, None, None, None, None);
    let (mut diff, mut upsert, mut before, mut at) = (false, false, None, None);
    let mut positional = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--dry" => dry = true,
            "--diff" => diff = true,
            "--upsert" => upsert = true,
            "--before" => before = Some(value(&mut it, &gen_usage)?),
            "--at" => at = Some(parse_line(&value(&mut it, &gen_usage)?)?),
            "--write" => write = Some(value(&mut it, &gen_usage)?),
            "--into" => into = Some(value(&mut it, &gen_usage)?),
            "--after" => after = Some(value(&mut it, &gen_usage)?),
            "--replace" => replace = Some(value(&mut it, &gen_usage)?),
            "--expect" => expect = Some(value(&mut it, &gen_usage)?),
            "--project" => project = Some(value(&mut it, &gen_usage)?),
            // minden más (a kifejezés is, akkor is, ha `-`-lel kezdődik: `-sign()`) a kifejezés
            _ => positional.push(arg.clone()),
        }
    }
    let [expr] = positional.as_slice() else {
        return Err(CallgraphError::Usage(crate::tf!(
            "Exactly one expression expected, quote it as one argument. {}",
            "Pontosan egy kifejezés kell, idézőjelben, egy argumentumként. {}",
            gen_usage()
        )));
    };
    let targets =
        usize::from(write.is_some()) + usize::from(into.is_some()) + usize::from(replace.is_some());
    if expect.is_some() && replace.is_none() && !(upsert && into.is_some()) {
        return Err(CallgraphError::Usage(crate::tf!(
            "--expect only works with --replace (or --into --upsert, checked only when the member exists): it is the sha256 prefix of the replaced lines (not of the whole file); a wrong value prints the current one. {}",
            "Az --expect csak a --replace-szel (vagy --into --upsert-tel, csak ha a tag már létezik) megy: a lecserélt sorok (nem a teljes fájl) sha256-előtagja; hibás értéknél kiírja a jelenlegit. {}",
            gen_usage()
        )));
    }
    if (after.is_some() || before.is_some() || at.is_some() || upsert) && into.is_none() {
        return Err(CallgraphError::Usage(crate::tf!(
            "--after, --before, --at and --upsert only work with --into. {}",
            "Az --after, --before, --at és --upsert csak az --into-val megy. {}",
            gen_usage()
        )));
    }
    if usize::from(after.is_some()) + usize::from(before.is_some()) + usize::from(at.is_some()) > 1
    {
        return Err(CallgraphError::Usage(crate::tf!(
            "Only one place: --after, --before or --at. {}",
            "Csak egy hely adható meg: --after, --before vagy --at. {}",
            gen_usage()
        )));
    }
    if targets > 1 {
        return Err(CallgraphError::Usage(crate::tf!(
            "Only one target: --write, --into or --replace. {}",
            "Csak egy cél adható meg: --write, --into vagy --replace. {}",
            gen_usage()
        )));
    }
    let target = if let Some(file) = write {
        Target::Write { file }
    } else if let Some(spec) = into {
        let (file, class) = match spec.split_once("::") {
            Some((file, class)) => (file.to_owned(), Some(class.to_owned())),
            None => (spec, None),
        };
        Target::Into {
            file,
            class,
            place: Place::of(after, before, at),
            upsert,
            expect,
        }
    } else if let Some(spec) = replace {
        let (file, at) = parse_replace(&spec)?;
        Target::Replace { file, at, expect }
    } else {
        Target::Preview
    };
    Ok(CliArgs::Generate {
        request: GenRequest {
            expr: expr.clone(),
            // a --diff csak előnézet: semmi nem íródik
            mode: Mode::of(dry, diff),
            target,
        },
        project,
    })
}

/// `undo [<fájl>] [--project X]`.
pub(crate) fn parse_undo(args: &[String]) -> Result<CliArgs> {
    let mut project = None;
    let mut files = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--project" => project = Some(value(&mut it, &undo_usage)?),
            _ => files.push(arg.clone()),
        }
    }
    if files.len() > 1 {
        return Err(usage_error(undo_usage()));
    }
    Ok(CliArgs::Undo {
        file: files.pop(),
        project,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    fn request(args: &[&str]) -> GenRequest {
        match parse_gen(&s(args)).unwrap() {
            CliArgs::Generate { request, .. } => request,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn gen_without_target_is_a_preview() {
        let r = request(&["c:A"]);
        assert_eq!(r.target, Target::Preview);
        assert_eq!(r.mode, Mode::Write);
        assert_eq!(request(&["c:A", "--dry"]).mode, Mode::Dry);
    }

    #[test]
    fn gen_targets_parse() {
        assert_eq!(
            request(&["c:A", "--write", "app/A.php"]).target,
            Target::Write {
                file: "app/A.php".into()
            }
        );
        assert_eq!(
            request(&["+x()", "--into", "app/A.php::A", "--after", "go"]).target,
            Target::Into {
                file: "app/A.php".into(),
                class: Some("A".into()),
                place: Place::After("go".into()),
                upsert: false,
                expect: None
            }
        );
        assert_eq!(
            request(&["+x()", "--replace", "app/A.php::go", "--expect", "ab12"]).target,
            Target::Replace {
                file: "app/A.php".into(),
                at: ReplaceAt::Method {
                    class: None,
                    name: "go".into()
                },
                expect: Some("ab12".into())
            }
        );
        assert_eq!(
            request(&["+x()", "--replace", "app/A.php:L42-L87"]).target,
            Target::Replace {
                file: "app/A.php".into(),
                at: ReplaceAt::Lines(42, 87),
                expect: None
            }
        );
        assert_eq!(
            request(&["+x()", "--replace", "app/A.php:3-4"]).target,
            Target::Replace {
                file: "app/A.php".into(),
                at: ReplaceAt::Lines(3, 4),
                expect: None
            }
        );
    }

    #[test]
    fn diff_before_at_and_upsert_parse() {
        let r = request(&["+x()", "--replace", "a.php::go", "--diff"]);
        assert_eq!(r.mode, Mode::Diff);
        let r = request(&["+x()", "--into", "a.php::A", "--before", "go", "--upsert"]);
        assert_eq!(r.mode, Mode::Write);
        assert!(matches!(
            r.target,
            Target::Into { place: Place::Before(ref m), upsert: true, .. } if m == "go"
        ));
        let r = request(&["+x()", "--into", "a.php", "--at", "L42"]);
        assert!(matches!(
            r.target,
            Target::Into {
                place: Place::Line(42),
                ..
            }
        ));
        let r = request(&["+x()", "--into", "a.vue::template#list"]);
        assert_eq!(
            r.target,
            Target::Into {
                file: "a.vue".into(),
                class: Some("template#list".into()),
                place: Place::End,
                upsert: false,
                expect: None
            }
        );
    }

    #[test]
    fn a_member_expression_may_start_with_a_dash() {
        assert_eq!(request(&["-sign():string", "--dry"]).expr, "-sign():string");
    }

    #[test]
    fn gen_rejects_bad_combinations() {
        for bad in [
            &[][..],
            &["c:A", "c:B"],
            &["c:A", "--write", "a.php", "--into", "b.php"],
            &["c:A", "--after", "x"],
            &["c:A", "--upsert"],
            &["+x()", "--into", "a.php", "--after", "x", "--before", "y"],
            &["+x()", "--into", "a.php", "--at", "x"],
            &["+x()", "--into", "a.php", "--at", "0"],
            &["c:A", "--expect", "ab"],
            &["c:A", "--write"],
            &["c:A", "--replace", "a.php"],
            &["c:A", "--replace", "a.php:L9"],
        ] {
            assert!(parse_gen(&s(bad)).is_err(), "{bad:?}");
        }
        let err = parse_gen(&s(&["c:A", "--into", "a.php", "--expect", "ab"]))
            .unwrap_err()
            .to_string();
        assert!(err.contains("--replace"), "{err}");
        // --upsert-tel az --expect megengedett (csere esetén ellenőriz)
        let r = request(&["+x()", "--into", "a.php", "--upsert", "--expect", "ab12"]);
        assert!(
            matches!(r.target, Target::Into { upsert: true, expect: Some(ref e), .. } if e == "ab12"),
            "{:?}",
            r.target
        );
    }

    #[test]
    fn undo_parses() {
        assert_eq!(
            parse_undo(&s(&[])).unwrap(),
            CliArgs::Undo {
                file: None,
                project: None
            }
        );
        assert_eq!(
            parse_undo(&s(&["app/A.php", "--project", "x"])).unwrap(),
            CliArgs::Undo {
                file: Some("app/A.php".into()),
                project: Some("x".into())
            }
        );
        assert!(parse_undo(&s(&["a", "b"])).is_err());
    }

    /// A CLI-hiba végén a használati sor áll; az MCP-s változatban nincs, de az üzenet marad.
    #[test]
    fn compact_usage_drops_only_the_trailing_usage_line() {
        let Err(CallgraphError::Usage(text)) =
            parse_gen(&s(&["c:A", "--write", "a.php", "--into", "b.php"]))
        else {
            panic!("usage error expected");
        };
        assert!(text.ends_with(&gen_usage()), "{text}");
        let compact = compact_usage(&text);
        assert!(!compact.contains("callgraph gen"), "{compact}");
        assert!(compact.contains("--write"), "{compact}");
        // a csak-használati-sor hiba (hiányzó érték) és az idegen szöveg változatlan
        assert_eq!(compact_usage(&gen_usage()), gen_usage());
        assert_eq!(compact_usage("something else"), "something else");
        let Err(CallgraphError::Usage(text)) = parse_undo(&s(&["a", "b"])) else {
            panic!("usage error expected");
        };
        assert_eq!(compact_usage(&text), text);
    }
}
