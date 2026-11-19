use super::*;
use crate::core::SourceFile;
use crate::lang::Language;
use crate::lang::js::JsFrontend;

/// (component, `name*` list, flags) of the facts of one file.
fn facts(path: &str, text: &str) -> Vec<(String, Vec<String>, String)> {
    let a = JsFrontend::with_defaults()
        .analyze(&SourceFile::new(path, text.as_bytes().to_vec(), None))
        .unwrap();
    let c = &a.graph.constants;
    c.iter()
        .filter(|k| k.kind == COMP_KIND)
        .map(|k| {
            let props = c
                .iter()
                .filter(|p| p.kind == PROP_KIND)
                .filter_map(|p| {
                    let name = p.name.strip_prefix(&format!("{}.", k.name))?;
                    Some(format!(
                        "{name}{}",
                        if p.value.is_some() { "*" } else { "" }
                    ))
                })
                .collect();
            (k.name.clone(), props, k.value.clone().unwrap_or_default())
        })
        .collect()
}

fn one(path: &str, text: &str) -> (Vec<String>, String) {
    let all = facts(path, text);
    assert_eq!(all.len(), 1, "{all:?}");
    (all[0].1.clone(), all[0].2.clone())
}

#[test]
fn destructured_defaults_and_rest() {
    let (props, flags) = one(
        "/p/Card.jsx",
        "export function Card({ title, size = 'md', onSave }) { return <div>{title}</div>; }\n",
    );
    assert_eq!(props, ["title", "size", "onSave"]);
    assert_eq!(flags, "export:Card");
    let (props, flags) = one(
        "/p/Card.jsx",
        "export default function Card({ title, ...rest }) { return <div {...rest}>{title}</div>; }\n",
    );
    assert_eq!(props, ["title"]);
    assert_eq!(flags, "default,open");
}

#[test]
fn typed_props_mark_required() {
    let (props, _) = one(
        "/p/Card.tsx",
        "interface Props { title: string; size?: number; onSave: () => void }\nexport const Card = ({ title, size = 3 }: Props) => <p>{title}</p>;\n",
    );
    assert_eq!(props, ["title*", "size", "onSave*"]);
}

#[test]
fn fc_memo_and_forward_ref_wrappers() {
    let (props, _) = one(
        "/p/A.tsx",
        "import React from 'react';\ntype P = { a: string; b?: string };\nexport const A: React.FC<P> = ({ a }) => <i>{a}</i>;\n",
    );
    assert_eq!(props, ["a*", "b", "children"]);
    let (props, flags) = one(
        "/p/B.tsx",
        "import { memo } from 'react';\ntype P = { a: string };\nconst B = memo((p: P) => <i>{p.a}</i>);\nexport default B;\n",
    );
    assert_eq!(props, ["a*"]);
    assert_eq!(flags, "default");
    let (props, _) = one(
        "/p/C.tsx",
        "import { forwardRef } from 'react';\ninterface P { a: string }\nexport const C = forwardRef<HTMLDivElement, P>((props, ref) => <div ref={ref}>{props.a}</div>);\n",
    );
    assert_eq!(props, ["a*"]);
}

#[test]
fn unresolved_or_html_types_are_open() {
    let (props, flags) = one(
        "/p/B.tsx",
        "import type { ButtonHTMLAttributes } from 'react';\ninterface P extends ButtonHTMLAttributes<HTMLButtonElement> { label: string }\nexport function B({ label }: P) { return <button>{label}</button>; }\n",
    );
    assert_eq!(props, ["label*"]);
    assert!(flags.contains("open"), "{flags}");
    let (_, flags) = one(
        "/p/U.tsx",
        "type P = Omit<Base, 'x'> & { a: string };\nexport function U(p: P) { return <i>{p.a}</i>; }\n",
    );
    assert!(flags.contains("open"), "{flags}");
}

#[test]
fn plain_props_object_reads_the_accesses_and_is_open() {
    let (props, flags) = one(
        "/p/L.jsx",
        "export function L(props) { return <li>{props.name}{props.id}</li>; }\n",
    );
    assert_eq!(props, ["name", "id"]);
    assert!(flags.contains("open"));
}

#[test]
fn prop_types_and_default_props() {
    let (props, flags) = one(
        "/p/P.jsx",
        "export function P({ a, b, c }) { return <i>{a}{b}{c}</i>; }\nP.propTypes = { a: PropTypes.string.isRequired, b: PropTypes.number, c: PropTypes.shape({ x: PropTypes.string.isRequired }).isRequired };\nP.defaultProps = { c: {} };\n",
    );
    assert_eq!(props, ["a*", "b", "c"]);
    assert!(!flags.contains("open"), "{flags}");
}

#[test]
fn imported_props_type_is_read() {
    let dir = std::env::temp_dir().join(format!("cg-react-types-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("types.ts"),
        "export interface BoxProps { w: number; h?: number }\n",
    )
    .unwrap();
    let file = dir.join("Box.tsx");
    let text = "import type { BoxProps } from './types';\nexport function Box({ w }: BoxProps) { return <div>{w}</div>; }\n";
    std::fs::write(&file, text).unwrap();
    let (props, flags) = one(file.to_str().unwrap(), text);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(props, ["w*", "h"]);
    assert!(!flags.contains("open"), "{flags}");
}

#[test]
fn non_components_are_not_listed() {
    // lowercase, no JSX, or a `.ts` file
    assert!(
        facts(
            "/p/a.jsx",
            "export function helper({ a }) { return <i>{a}</i>; }\n"
        )
        .is_empty()
    );
    assert!(facts("/p/a.jsx", "export function Parse({ a }) { return a; }\n").is_empty());
    assert!(facts("/p/a.ts", "export function Parse({ a }) { return 1; }\n").is_empty());
}
