//! A `rename` parancs kapcsolói:
//! `callgraph rename <route|class|method> <régi> <új> [--dry] [--strings] [--keep-table] [--project alias|mappa]`.
//! Külön fájlban, hogy az `args.rs` csak egy-egy sorral bővüljön.

use crate::error::{CallgraphError, Result};
use crate::rename::{Kind, RenameRequest};

use super::CliArgs;

fn rename_usage() -> String {
    crate::t!(
        "callgraph rename <route|class|method> <old> <new> [--dry] [--strings] [--keep-table] [--project alias]  (e.g. rename route profile.edit account.edit | rename route 'profile.*' 'account.*' | rename class 'App\\Models\\Fee' 'App\\Models\\Charge' | rename method 'App\\Services\\Pay::charge' bill; one `callgraph undo` reverts it)",
        "callgraph rename <route|class|method> <régi> <új> [--dry] [--strings] [--keep-table] [--project alias]  (pl. rename route profile.edit account.edit | rename route 'profile.*' 'account.*' | rename class 'App\\Models\\Fee' 'App\\Models\\Charge' | rename method 'App\\Services\\Pay::charge' bill; egy `callgraph undo` visszavonja)"
    )
    .to_owned()
}

/// `rename <fajta> <régi> <új> [kapcsolók]`.
pub(crate) fn parse_rename(args: &[String]) -> Result<CliArgs> {
    let usage = || CallgraphError::Usage(rename_usage());
    let (mut dry, mut strings, mut keep_table, mut project) = (false, false, false, None);
    let mut positional: Vec<&String> = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--dry" => dry = true,
            "--strings" => strings = true,
            "--keep-table" => keep_table = true,
            "--project" => project = Some(it.next().ok_or_else(usage)?.clone()),
            _ => positional.push(arg),
        }
    }
    let [kind, old, new] = positional.as_slice() else {
        return Err(usage());
    };
    let kind = Kind::parse(kind).ok_or_else(usage)?;
    Ok(CliArgs::Rename {
        request: RenameRequest {
            kind,
            old: (*old).clone(),
            new: (*new).clone(),
            dry,
            strings,
            keep_table,
        },
        project,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn parses_kind_names_and_flags() {
        let parsed = parse_rename(&s(&["route", "a.b", "c.d", "--dry", "--project", "x"])).unwrap();
        let CliArgs::Rename { request, project } = parsed else {
            panic!("rename expected");
        };
        assert_eq!(request.kind, Kind::Route);
        assert_eq!((request.old.as_str(), request.new.as_str()), ("a.b", "c.d"));
        assert!(request.dry && !request.strings && !request.keep_table);
        assert_eq!(project.as_deref(), Some("x"));
    }

    #[test]
    fn rejects_unknown_kinds_and_missing_names() {
        assert!(parse_rename(&s(&["table", "a", "b"])).is_err());
        assert!(parse_rename(&s(&["route", "a"])).is_err());
        assert!(parse_rename(&s(&["route", "a", "b", "c"])).is_err());
    }
}
