//! The second `--depth` level: several references from one top-level caller stay separate rows.

use super::CallerIndex;
use crate::model::{Call, FileGraph, FileGraphBuilder};

fn graph(file: &str, namespace: &str, calls: Vec<Call>) -> FileGraph {
    let mut b = FileGraphBuilder::new(file);
    b.namespace_if_unset(Some(namespace.to_owned()));
    for call in calls {
        b.call(call);
    }
    b.build()
}

fn call(target: &str, resolved: &str, line: u32, container: Option<&str>, caller: &str) -> Call {
    Call::new("static_call", target, "external_call")
        .at(line)
        .within(container.map(str::to_owned), Some(caller.to_owned()))
        .resolved(Some(resolved.to_owned()))
}

#[test]
fn top_level_caller_keeps_one_row_per_referenced_target() {
    let blog = graph(
        "/p/BlogController.php",
        "App\\Http",
        vec![
            call(
                "Article::published",
                "App\\Article::published",
                17,
                Some("BlogController"),
                "list",
            ),
            call(
                "Article::published",
                "App\\Article::published",
                38,
                Some("BlogController"),
                "show",
            ),
        ],
    );
    let routes = graph(
        "/p/routes.php",
        "",
        vec![
            call(
                "BlogController::list",
                "App\\Http\\BlogController::list",
                41,
                None,
                "__top__",
            ),
            call(
                "BlogController::show",
                "App\\Http\\BlogController::show",
                42,
                None,
                "__top__",
            ),
        ],
    );
    let index = CallerIndex::build([("/p/BlogController.php", &blog), ("/p/routes.php", &routes)]);
    let first = index.callers("Article::published", 1).unwrap();
    assert_eq!(first.len(), 2);
    let (rows, _) = index.parents_of_with_skips(&first, 2, false);
    let mut lines: Vec<(u32, &str)> = rows
        .iter()
        .filter(|row| row.level == 2)
        .map(|row| (row.line, row.target.as_str()))
        .collect();
    lines.sort();
    assert_eq!(
        lines,
        vec![
            (41, "App\\Http\\BlogController::list"),
            (42, "App\\Http\\BlogController::show"),
        ]
    );
}
