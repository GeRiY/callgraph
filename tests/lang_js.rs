//! A JS / TS / Vue frontend regisztrációja.

use std::path::Path;

use callgraph::lang::{LanguageRegistry, LanguageRole};

#[test]
fn registry_routes_js_family_to_frontend() {
    let r = LanguageRegistry::with_defaults();
    for ext in ["js", "mjs", "cjs", "ts", "tsx", "jsx", "vue"] {
        let l = r.for_path(Path::new(&format!("a/b.{ext}"))).unwrap();
        assert_eq!((l.id(), l.role()), ("js", LanguageRole::Frontend), "{ext}");
    }
}
