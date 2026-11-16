//! SCSS frontend: Raffia-validálás és SCSS → `FileGraph` leképezés.

mod frontend;

pub use frontend::ScssFrontend;

#[cfg(test)]
mod tests {
    use super::ScssFrontend;
    use crate::core::SourceFile;
    use crate::lang::Language;

    /// Az `@include` / függvényhívás `caller`-e a befoglaló szabály vagy mixin; szabályon kívül `None`.
    #[test]
    fn calls_get_the_enclosing_rule_as_caller() {
        let source = "@mixin pad($n) {
  padding: map-get($sizes, $n);
}
.card {
  @include pad(2);
  color: darken($brand, 10%);
}
.chip { @include pad(1); }
";
        let file = SourceFile::new("/tmp/scss/app.scss", source.as_bytes().to_vec(), None);
        let graph = ScssFrontend::new().analyze(&file).unwrap().graph;
        let callers = |kind: &str, target: &str| -> Vec<Option<&str>> {
            graph
                .calls
                .iter()
                .filter(|c| c.kind == kind && c.target == target)
                .map(|c| c.caller.as_deref())
                .collect()
        };
        assert_eq!(callers("func_call", "map-get"), [Some("pad")]);
        assert_eq!(callers("include", "pad"), [Some(".card"), Some(".chip")]);
        assert_eq!(callers("func_call", "darken"), [Some(".card")]);
    }
}
