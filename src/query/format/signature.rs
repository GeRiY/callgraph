//! Metódus-szignatúra az outline-hoz (`--signatures`) és az api-hoz: `name(params): ret`.

use crate::model::Method;

/// A szignatúra szövege: `name(p1, p2): ret`; a visszatérési típus nélkül `name(p1, p2)`.
pub(super) fn signature(m: &Method) -> String {
    let mut s = format!("{}({})", m.name, m.params.join(", "));
    if let Some(ret) = &m.return_type {
        s.push_str(": ");
        s.push_str(ret);
    }
    s
}

/// A docblock-típusok szuffixuma (` @return array{…}`), ha vannak.
pub(super) fn doc_types_suffix(m: &Method) -> String {
    m.doc_types
        .as_ref()
        .map(|t| format!(" {t}"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn method(params: &[&str], ret: Option<&str>) -> Method {
        Method {
            container: None,
            name: "run".into(),
            line: 1,
            end_line: Some(1),
            visibility: None,
            is_static: false,
            is_abstract: false,
            kind: None,
            params: params.iter().map(|p| (*p).to_owned()).collect(),
            return_type: ret.map(str::to_owned),
            doc_types: Some("@return list<int>".into()),
            doc: None,
            body: None,
        }
    }

    #[test]
    fn signature_joins_params_and_return_type() {
        assert_eq!(
            signature(&method(&["int $a", "bool $b = false"], Some("array"))),
            "run(int $a, bool $b = false): array"
        );
        assert_eq!(signature(&method(&[], None)), "run()");
        assert_eq!(doc_types_suffix(&method(&[], None)), " @return list<int>");
    }
}
