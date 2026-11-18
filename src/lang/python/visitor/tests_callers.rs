//! `self.` / `cls.` calls through the caller index: the `Type::method` search and the second `--depth` level.

use crate::core::SourceFile;
use crate::index::CallerIndex;
use crate::lang::Language;
use crate::lang::python::PythonFrontend;
use crate::model::FileGraph;

fn graph(path: &str, text: &str) -> FileGraph {
    PythonFrontend::with_defaults()
        .analyze(&SourceFile::new(path, text.as_bytes().to_vec(), None))
        .unwrap()
        .graph
}

const APP: &str = r#"class Flask:
    def make_response(self, rv):
        return rv

    def finalize_request(self, rv):
        return self.make_response(rv)

    def full_dispatch_request(self):
        return self.finalize_request(1)

    def handle_exception(self):
        return self.finalize_request(2)

    def unrelated(self):
        return self.repo.make_response(3)

    @classmethod
    def build(cls):
        return cls.make_response(cls, 4)
"#;

fn callers_of(index: &CallerIndex, pattern: &str) -> Vec<String> {
    let mut names: Vec<String> = index
        .callers(pattern, 1)
        .unwrap()
        .into_iter()
        .map(|row| row.caller)
        .collect();
    names.sort();
    names
}

#[test]
fn self_call_is_found_by_the_type_method_pattern() {
    let app = graph("/p/app.py", APP);
    let index = CallerIndex::build([("/p/app.py", &app)]);
    // `self.repo.make_response` is another receiver: it is not a call of `Flask::make_response`
    assert_eq!(
        callers_of(&index, "Flask::make_response"),
        vec!["build".to_owned(), "finalize_request".to_owned()]
    );
}

#[test]
fn depth_two_follows_self_calls() {
    let app = graph("/p/app.py", APP);
    let index = CallerIndex::build([("/p/app.py", &app)]);
    let first = index.callers("Flask::make_response", 1).unwrap();
    let (rows, _) = index.parents_of_with_skips(&first, 2, false);
    let mut level_two: Vec<&str> = rows
        .iter()
        .filter(|row| row.level == 2)
        .map(|row| row.caller.as_str())
        .collect();
    level_two.sort();
    assert_eq!(level_two, vec!["full_dispatch_request", "handle_exception"]);
}
