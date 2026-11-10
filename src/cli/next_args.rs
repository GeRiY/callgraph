//! A `next` parancs kapcsolói: `callgraph next <rN> <k>[,<k>…] [--project alias|mappa]` (egy korábbi eredmény
//! javasolt következő lépése az azonosítójával, `crate::next`). Külön fájlban, hogy az `args.rs` csak egy sorral
//! bővüljön.

use crate::error::{CallgraphError, Result};

use super::CliArgs;

fn next_usage() -> String {
    crate::t!(
        "callgraph next <result-id> <choice>[,<choice>] [--project alias]  (e.g. next r7 1 or next r7 1,3: runs suggestion 1 (and 3) of result r7)",
        "callgraph next <eredmény-id> <választás>[,<választás>] [--project alias]  (pl. next r7 1 vagy next r7 1,3: az r7 eredmény 1. (és 3.) javaslatát futtatja)"
    )
    .to_owned()
}

/// `next <rN> <k>[,<k>] [--project X]`.
pub(crate) fn parse_next(args: &[String]) -> Result<CliArgs> {
    let mut positional = Vec::new();
    let mut project = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--project" => {
                project = Some(
                    it.next()
                        .cloned()
                        .ok_or_else(|| CallgraphError::Usage(next_usage()))?,
                );
            }
            _ => positional.push(a.clone()),
        }
    }
    let [id, choice] = positional.as_slice() else {
        return Err(CallgraphError::Usage(next_usage()));
    };
    if crate::next::parse_id(id).is_none() {
        return Err(CallgraphError::Usage(crate::tf!(
            "Not a result id: {id} (expected e.g. r7). {}",
            "Nem eredmény-azonosító: {id} (pl. r7 a jó alak). {}",
            next_usage()
        )));
    }
    let choices = crate::next::parse_choices(choice).ok_or_else(|| {
        CallgraphError::Usage(crate::tf!(
            "Not a choice: {choice} (expected 1, 2, 3 or a list like 1,3). {}",
            "Nem választás: {choice} (1, 2, 3 vagy lista, pl. 1,3). {}",
            next_usage()
        ))
    })?;
    Ok(CliArgs::Next {
        id: id.clone(),
        choices,
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
    fn parses_next_id_and_choices() {
        assert_eq!(
            parse_next(&s(&["r7", "1"])).unwrap(),
            CliArgs::Next {
                id: "r7".into(),
                choices: vec![1],
                project: None
            }
        );
        assert_eq!(
            parse_next(&s(&["r7", "1,3", "--project", "sp"])).unwrap(),
            CliArgs::Next {
                id: "r7".into(),
                choices: vec![1, 3],
                project: Some("sp".into())
            }
        );
        for bad in [
            &["7", "1"][..],
            &["rx", "1"],
            &["r7", "4"],
            &["r7", "0"],
            &["r7", "a"],
            &["r7"],
            &["r7", "1", "2"],
            &["r7", "1", "--project"],
        ] {
            assert!(parse_next(&s(bad)).is_err(), "{bad:?}");
        }
    }
}
