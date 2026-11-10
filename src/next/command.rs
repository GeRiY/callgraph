//! A javaslatok futtatható argumentumai (`show --project <gyökér> …`, abszolút utakkal) és címkéi.

use std::path::Path;

use super::suggestion::{Suggestion, SuggestionKind};

pub(super) fn outline_of(root: &Path, path: &str, kind: SuggestionKind) -> Suggestion {
    let abs = absolute(root, path);
    Suggestion {
        kind,
        label: crate::tf!("outline {}", "outline {}", relative(root, &abs)),
        args: show_args(root, &[], &[abs]),
    }
}

pub(super) fn body_of(root: &Path, path: &str, name: &str, kind: SuggestionKind) -> Suggestion {
    let abs = absolute(root, path);
    Suggestion {
        kind,
        label: format!("{}::{name} --body", relative(root, &abs)),
        args: show_args(root, &["--body".to_owned()], &[format!("{abs}::{name}")]),
    }
}

pub(super) fn callers_of(root: &Path, name: &str) -> Suggestion {
    let pattern = format!(r"(^|::|\.|->){}$", regex::escape(name));
    Suggestion {
        kind: SuggestionKind::Callers,
        label: crate::tf!("callers of {name}", "{name} hívói"),
        args: show_args(
            root,
            &["--grep".to_owned(), pattern],
            &[root.display().to_string()],
        ),
    }
}

/// `show --project <gyökér> --label <gyökér> <kapcsolók> <célok>`.
pub(super) fn show_args(root: &Path, flags: &[String], targets: &[String]) -> Vec<String> {
    let root = root.display().to_string();
    let mut args = vec![
        "show".to_owned(),
        "--project".to_owned(),
        root.clone(),
        "--label".to_owned(),
        root,
    ];
    args.extend(flags.iter().cloned());
    args.extend(targets.iter().cloned());
    args
}

/// Abszolút út: ha már az, változatlan; különben a `base`-hez képest.
pub(super) fn absolute(base: &Path, path: &str) -> String {
    if Path::new(path).is_absolute() {
        path.to_owned()
    } else {
        base.join(path).display().to_string()
    }
}

/// A gyökérhez relatív alak (a címkében); a gyökéren kívül változatlan.
pub(super) fn relative(root: &Path, path: &str) -> String {
    Path::new(path)
        .strip_prefix(root)
        .map_or_else(|_| path.to_owned(), |p| p.display().to_string())
}
