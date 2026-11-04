use callgraph::query::{OutputLimit, SelectorSet};
use std::path::PathBuf;

#[test]
fn selector_rejects_mixed_arguments() {
    let args = vec![PathBuf::from("a.php::one"), PathBuf::from("b.php")];
    assert!(SelectorSet::parse(&args).is_err());
}

#[test]
fn selector_groups_methods_by_file() {
    let args = vec![PathBuf::from("a.php::one"), PathBuf::from("a.php::two")];
    let selectors = SelectorSet::parse(&args).unwrap();
    assert_eq!(
        selectors
            .methods_for(std::path::Path::new("a.php"))
            .unwrap(),
        ["one", "two"]
    );
}

#[test]
fn output_limit_can_be_overridden() {
    let limited = OutputLimit::from_env(false).with_value(Some(1));
    assert!(
        limited
            .method_index("a\nb".into(), "index".into())
            .starts_with('⚠')
    );
}
