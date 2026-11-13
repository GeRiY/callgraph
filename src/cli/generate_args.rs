//! A `gen` és az `undo` parancs kapcsolói:
//! `callgraph gen '<kifejezés>' [--dry] [--write <fájl> | --into <fájl>[::Osztály] [--after <metódus>] |
//! --replace <fájl>(:L<a>-L<b> | ::<metódus>) [--expect <sha256-előtag>]] [--project alias|mappa]`,
//! `callgraph undo [<fájl>] [--project alias|mappa]`. Külön fájlban, hogy az `args.rs` csak egy-egy sorral bővüljön.

use crate::error::{CallgraphError, Result};
use crate::generate::{GenRequest, ReplaceAt, Target};

use super::CliArgs;

fn gen_usage() -> String {
    crate::t!(
        "callgraph gen '<expr>' [--dry] [--write <file> | --into <file>[::Class] [--after <method>] | --replace <file>(:L<a>-L<b> | ::<method>) [--expect <sha256-prefix>]] [--project alias]  (e.g. gen 'c!:Pay<Base> > ctor(-Client $http) + +charge(Order $o):Receipt' --write app/Pay.php)",
        "callgraph gen '<kifejezés>' [--dry] [--write <fájl> | --into <fájl>[::Osztály] [--after <metódus>] | --replace <fájl>(:L<a>-L<b> | ::<metódus>) [--expect <sha256-előtag>]] [--project alias]  (pl. gen 'c!:Pay<Base> > ctor(-Client $http) + +charge(Order $o):Receipt' --write app/Pay.php)"
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

/// `gen '<kifejezés>' [kapcsolók]`.
pub(crate) fn parse_gen(args: &[String]) -> Result<CliArgs> {
    let (mut dry, mut project) = (false, None);
    let (mut write, mut into, mut after, mut replace, mut expect) = (None, None, None, None, None);
    let mut positional = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--dry" => dry = true,
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
    let targets = usize::from(write.is_some())
        + usize::from(into.is_some())
        + usize::from(replace.is_some());
    if targets > 1 || (after.is_some() && into.is_none()) || (expect.is_some() && replace.is_none())
    {
        return Err(usage_error(gen_usage()));
    }
    let target = if let Some(file) = write {
        Target::Write { file }
    } else if let Some(spec) = into {
        let (file, class) = match spec.split_once("::") {
            Some((file, class)) => (file.to_owned(), Some(class.to_owned())),
            None => (spec, None),
        };
        Target::Into { file, class, after }
    } else if let Some(spec) = replace {
        let (file, at) = parse_replace(&spec)?;
        Target::Replace { file, at, expect }
    } else {
        Target::Preview
    };
    Ok(CliArgs::Generate {
        request: GenRequest {
            expr: expr.clone(),
            dry,
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
        assert!(!r.dry);
        assert!(request(&["c:A", "--dry"]).dry);
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
                after: Some("go".into())
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
            &["c:A", "--expect", "ab"],
            &["c:A", "--write"],
            &["c:A", "--replace", "a.php"],
            &["c:A", "--replace", "a.php:L9"],
        ] {
            assert!(parse_gen(&s(bad)).is_err(), "{bad:?}");
        }
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
}
