//! The `impact` command's switches: `callgraph impact [--diff [<range>|-]] [--staged] [--depth N] [--project <alias|dir>]
//! [--compact] [--format text|json] [--full]` (what the current change affects). A separate file so `args.rs` only grows
//! by a line or two.

use crate::error::{CallgraphError, Result};
use crate::impact::{DiffSource, MAX_DEPTH};

use super::CliArgs;

/// The command usage (shared by the CLI and the error message).
pub(crate) fn impact_usage() -> &'static str {
    crate::t!(
        "callgraph impact [--diff [<range>|-]] [--staged] [--depth 1-3] [--project <alias|dir>] [--compact] [--format text|json] [--full]  (the changed symbols of the git diff with their callers, routes, pages and tests; default: working tree + index vs HEAD; --staged: the index; --diff main...HEAD or HEAD~3: a range; --diff -: a unified diff on stdin)",
        "callgraph impact [--diff [<tartomány>|-]] [--staged] [--depth 1-3] [--project <alias|mappa>] [--compact] [--format text|json] [--full]  (a git diff módosult szimbólumai a hívóikkal, route-jaikkal, oldalaikkal és teszteikkel; alapból: munkafa + index a HEAD-hez képest; --staged: az index; --diff main...HEAD vagy HEAD~3: tartomány; --diff -: unified diff az stdin-en)"
    )
}

/// The value of a switch: the `--flag=value` part, or the next argument (the index moves past it).
fn value_of(inline: &Option<String>, args: &[String], at: &mut usize) -> Option<String> {
    match inline {
        Some(value) => Some(value.clone()),
        None => {
            *at += 1;
            args.get(*at).cloned()
        }
    }
}

/// `impact [--diff [<range>|-]] [--staged] [--depth N] [--project X] [--compact] [--format text|json] [--full]`.
pub(crate) fn parse_impact(args: &[String]) -> Result<CliArgs> {
    let usage = || CallgraphError::Usage(impact_usage().into());
    let mut diff: Option<DiffSource> = None;
    let (mut staged, mut compact, mut json, mut full) = (false, false, false, false);
    let mut depth = 1usize;
    let mut project: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v.to_owned())),
            _ => (arg, None),
        };
        match flag {
            "--diff" => match inline.as_deref() {
                Some("-") => diff = Some(DiffSource::Stdin),
                Some("") => return Err(usage()),
                Some(range) => diff = Some(DiffSource::Range(range.to_owned())),
                // the range is optional: a following switch is not a range
                None => match args.get(i + 1) {
                    Some(next) if next == "-" => {
                        diff = Some(DiffSource::Stdin);
                        i += 1;
                    }
                    Some(next) if !next.starts_with('-') => {
                        diff = Some(DiffSource::Range(next.clone()));
                        i += 1;
                    }
                    _ => diff = Some(DiffSource::WorkingTree),
                },
            },
            "--staged" | "--cached" if inline.is_none() => staged = true,
            "--compact" if inline.is_none() => compact = true,
            "--full" if inline.is_none() => full = true,
            "--depth" => {
                let n: usize = value_of(&inline, args, &mut i)
                    .and_then(|v| v.parse().ok())
                    .ok_or_else(usage)?;
                if !(1..=MAX_DEPTH).contains(&n) {
                    return Err(usage());
                }
                depth = n;
            }
            "--project" => {
                project = Some(
                    value_of(&inline, args, &mut i)
                        .filter(|v| !v.is_empty())
                        .ok_or_else(usage)?,
                );
            }
            "--format" => match value_of(&inline, args, &mut i).as_deref() {
                Some("json") => json = true,
                Some("text") => json = false,
                _ => return Err(usage()),
            },
            _ => return Err(usage()),
        }
        i += 1;
    }
    let source = match (staged, diff) {
        (true, None | Some(DiffSource::WorkingTree)) => DiffSource::Staged,
        (true, Some(_)) => return Err(usage()),
        (false, Some(source)) => source,
        (false, None) => DiffSource::WorkingTree,
    };
    Ok(CliArgs::Impact {
        source,
        depth,
        project,
        compact,
        json,
        full,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    fn source(args: &[&str]) -> DiffSource {
        match parse_impact(&s(args)).unwrap() {
            CliArgs::Impact { source, .. } => source,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_source_is_the_working_tree_a_range_stdin_or_the_index() {
        assert_eq!(source(&[]), DiffSource::WorkingTree);
        assert_eq!(source(&["--diff"]), DiffSource::WorkingTree);
        assert_eq!(source(&["--diff", "--compact"]), DiffSource::WorkingTree);
        assert_eq!(
            source(&["--diff", "main...HEAD"]),
            DiffSource::Range("main...HEAD".into())
        );
        assert_eq!(
            source(&["--diff=HEAD~3"]),
            DiffSource::Range("HEAD~3".into())
        );
        assert_eq!(source(&["--diff", "-"]), DiffSource::Stdin);
        assert_eq!(source(&["--staged"]), DiffSource::Staged);
        assert_eq!(source(&["--diff", "--staged"]), DiffSource::Staged);
    }

    #[test]
    fn switches_depth_project_and_format() {
        let parsed = parse_impact(&s(&[
            "--depth=2",
            "--project",
            "shop",
            "--compact",
            "--format",
            "json",
            "--full",
        ]))
        .unwrap();
        assert_eq!(
            parsed,
            CliArgs::Impact {
                source: DiffSource::WorkingTree,
                depth: 2,
                project: Some("shop".into()),
                compact: true,
                json: true,
                full: true,
            }
        );
    }

    #[test]
    fn bad_input_is_a_usage_error() {
        for bad in [
            &["--depth", "0"][..],
            &["--depth", "4"],
            &["--depth", "x"],
            &["--depth"],
            &["--project"],
            &["--format", "xml"],
            &["--diff", "main", "--staged"],
            &["--diff="],
            &["stray"],
            &["--bogus"],
        ] {
            assert!(parse_impact(&s(bad)).is_err(), "{bad:?}");
        }
    }
}
