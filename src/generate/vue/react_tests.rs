//! Tests of the React generator (`react:` components, `hook:`, `--into` a `.tsx`): memory file system, fixed clock.

use std::cell::Cell;
use std::path::Path;

use super::{VueGenerator, wants_vue};
use crate::generate::fs::Env;
use crate::generate::generator::{GenRequest, Mode, Output, Place, Target, undo};
use crate::generate::testing::{FixedClock, MemFs, ROOT};

const T0: u64 = 1_700_000_000;

fn go(fs: &MemFs, expr: &str, dry: bool, target: Target) -> Result<Output, String> {
    let clock = FixedClock(Cell::new(T0));
    let env = Env {
        root: Path::new(ROOT),
        fs,
        clock: &clock,
    };
    let request = GenRequest {
        expr: expr.to_owned(),
        mode: Mode::of(dry, false),
        target,
    };
    VueGenerator::new(env)
        .run(&request)
        .map_err(|e| e.to_string())
}

fn write(file: &str) -> Target {
    Target::Write {
        file: file.to_owned(),
    }
}

fn into(file: &str) -> Target {
    Target::Into {
        file: file.to_owned(),
        class: None,
        place: Place::End,
        upsert: false,
        expect: None,
    }
}

fn text(out: &Output) -> String {
    out.lines.join("\n")
}

const TS_PROJECT: &[(&str, &str)] = &[("tsconfig.json", "{}")];

const CARD: &str = "react:Card>props(title:str,size?:'sm'|'md'='md',children?)+ref:count:int=0+fn:inc():void{ setCount(count + 1) }+jsx>div.card>h1{{ title }}+button[@click=inc]{+1}";

#[test]
fn component_in_a_typescript_project_has_typed_props_state_and_handler() {
    let fs = MemFs::with(TS_PROJECT);
    let out = go(&fs, CARD, true, Target::Preview).unwrap();
    let code = text(&out);
    assert!(
        code.contains("import { type ReactNode, useState } from 'react';"),
        "{code}"
    );
    assert!(code.contains("interface CardProps {"), "{code}");
    assert!(code.contains("  title: string;"), "{code}");
    assert!(code.contains("  size?: 'sm' | 'md';"), "{code}");
    assert!(code.contains("  children?: ReactNode;"), "{code}");
    assert!(
        code.contains(
            "export default function Card({ title, size = 'md', children }: CardProps) {"
        ),
        "{code}"
    );
    assert!(
        code.contains("  const [count, setCount] = useState<number>(0);"),
        "{code}"
    );
    assert!(code.contains("  function inc(): void {"), "{code}");
    assert!(code.contains("  return ("), "{code}");
    assert!(code.contains("    <div className=\"card\">"), "{code}");
    assert!(code.contains("      <h1>{title}</h1>"), "{code}");
    assert!(
        code.contains("      <button onClick={inc}>+1</button>"),
        "{code}"
    );
    assert!(code.trim_end().ends_with('}'), "{code}");
    // a preview writes nothing
    assert!(fs.get("src/Card.tsx").is_none());
}

#[test]
fn component_in_a_javascript_project_has_no_types() {
    let fs = MemFs::default();
    let out = go(&fs, CARD, false, write("src/Card.jsx")).unwrap();
    assert!(text(&out).contains("src/Card.jsx"), "{}", text(&out));
    let file = fs.get("src/Card.jsx").unwrap();
    assert!(file.contains("import { useState } from 'react';"), "{file}");
    assert!(!file.contains("interface"), "{file}");
    assert!(!file.contains("ReactNode;"), "{file}");
    assert!(
        file.contains(
            "/** @param {{ title: string, size?: 'sm' | 'md', children?: import('react').ReactNode }} props */"
        ),
        "{file}"
    );
    assert!(
        file.contains("export default function Card({ title, size = 'md', children }) {"),
        "{file}"
    );
    assert!(
        file.contains("const [count, setCount] = useState(0);"),
        "{file}"
    );
    assert!(file.contains("function inc() {"), "{file}");
}

#[test]
fn the_extension_decides_typescript_for_a_written_component() {
    let fs = MemFs::default();
    go(
        &fs,
        "react:Tag>props(label:str)+jsx>span{{ label }}",
        false,
        write("src/Tag.tsx"),
    )
    .unwrap();
    let file = fs.get("src/Tag.tsx").unwrap();
    assert!(file.contains("interface TagProps {"), "{file}");
    assert!(file.contains("({ label }: TagProps)"), "{file}");
}

#[test]
fn style_is_sampled_from_the_existing_components() {
    // no semicolons, four-space indent, named exports
    let fs = MemFs::with(&[(
        "src/components/Box.jsx",
        "import React from 'react'\n\nexport function Box({ children }) {\n    return (\n        <div>{children}</div>\n    )\n}\n",
    )]);
    go(
        &fs,
        "react:Card>props(title:str)+ref:n:int=0+jsx>Box>h1{{ title }}",
        false,
        write("src/components/Card.jsx"),
    )
    .unwrap();
    let file = fs.get("src/components/Card.jsx").unwrap();
    assert!(!file.contains(';'), "{file}");
    assert!(
        file.contains("import { Box } from '@/components/Box'\n"),
        "{file}"
    );
    assert!(file.contains("export function Card({ title }) {"), "{file}");
    assert!(
        file.contains("\n    const [n, setN] = useState(0)\n"),
        "{file}"
    );
    assert!(
        file.contains("\n    return (\n        <Box>\n            <h1>{title}</h1>"),
        "{file}"
    );
}

#[test]
fn semicolons_and_default_exports_follow_the_samples() {
    let fs = MemFs::with(&[
        (
            "src/components/Box.tsx",
            "import React from 'react';\n\nexport default function Box() {\n  return <div />;\n}\n",
        ),
        ("tsconfig.json", "{}"),
    ]);
    let out = go(&fs, "react:Card>jsx>Box", true, Target::Preview).unwrap();
    let code = text(&out);
    assert!(
        code.contains("import Box from '@/components/Box';"),
        "{code}"
    );
    assert!(code.contains("export default function Card() {"), "{code}");
    assert!(code.contains("    <Box />"), "{code}");
    assert!(code.contains("  );"), "{code}");
}

#[test]
fn unknown_components_are_listed_and_inertia_tags_imported() {
    let fs = MemFs::with(TS_PROJECT);
    let out = go(
        &fs,
        "react:Nav>jsx>nav>Link[:href=\"route('home')\"]{Home}+Mystery",
        true,
        Target::Preview,
    )
    .unwrap();
    let code = text(&out);
    assert!(
        code.contains("import { Link } from '@inertiajs/react';"),
        "{code}"
    );
    assert!(
        code.contains("<Link href={route('home')}>Home</Link>"),
        "{code}"
    );
    assert_eq!(out.unresolved, vec!["Mystery".to_owned()]);
}

#[test]
fn lists_and_conditions_become_map_and_and() {
    let fs = MemFs::with(TS_PROJECT);
    let out = go(
        &fs,
        "react:List>props(items:list<Item>)+jsx>ul>li[v-for=\"item in items\" :key=item.id]{{ item.name }}^p[v-if=\"items.length === 0\"]{Nothing}",
        true,
        Target::Preview,
    )
    .unwrap();
    let code = text(&out);
    assert!(code.contains("{items.map((item) => ("), "{code}");
    assert!(
        code.contains("<li key={item.id}>{item.name}</li>"),
        "{code}"
    );
    assert!(code.contains("))}"), "{code}");
    assert!(code.contains("{items.length === 0 && ("), "{code}");
    assert!(code.contains("<p>Nothing</p>"), "{code}");
}

#[test]
fn emits_become_callback_props_and_form_uses_inertia() {
    let fs = MemFs::with(TS_PROJECT);
    let out = go(
        &fs,
        "react:Login>emits(save:int,close)+form(email,remember:bool=true)+on:mounted{ console.log(1) }+jsx>form[@submit=submit]>input[:value=form.data.email]",
        true,
        Target::Preview,
    )
    .unwrap();
    let code = text(&out);
    assert!(
        code.contains("  onSave?: (payload: number) => void;"),
        "{code}"
    );
    assert!(code.contains("  onClose?: () => void;"), "{code}");
    assert!(code.contains("({ onSave, onClose }: LoginProps)"), "{code}");
    assert!(
        code.contains("import { useEffect } from 'react';"),
        "{code}"
    );
    assert!(
        code.contains("import { useForm } from '@inertiajs/react';"),
        "{code}"
    );
    assert!(code.contains("const form = useForm({"), "{code}");
    assert!(code.contains("useEffect(() => {"), "{code}");
    assert!(code.contains("}, []);"), "{code}");
    assert!(code.contains("<form onSubmit={submit}>"), "{code}");
    assert!(code.contains("<input value={form.data.email} />"), "{code}");
}

#[test]
fn a_custom_hook_returns_its_state_and_functions() {
    let fs = MemFs::with(TS_PROJECT);
    let expr =
        "hook:useCart>ref:items:list<Item>=[]+fn:add(item:Item):void{ setItems([...items, item]) }";
    go(&fs, expr, false, write("src/hooks/useCart.ts")).unwrap();
    let file = fs.get("src/hooks/useCart.ts").unwrap();
    assert!(file.contains("import { useState } from 'react';"), "{file}");
    assert!(file.contains("export function useCart() {"), "{file}");
    assert!(
        file.contains("  const [items, setItems] = useState<Item[]>([]);"),
        "{file}"
    );
    assert!(
        file.contains("  function add(item: Item): void {"),
        "{file}"
    );
    assert!(
        file.contains("  return {\n    items,\n    setItems,\n    add,\n  };\n}"),
        "{file}"
    );
    // plain JS: no types
    go(&fs, expr, false, write("src/hooks/useBag.js")).unwrap();
    let js = fs.get("src/hooks/useBag.js").unwrap();
    assert!(js.contains("useState([])"), "{js}");
    assert!(js.contains("function add(item) {"), "{js}");
    // a hook takes no props
    let err = go(&fs, "hook:useX>props(a:str)", true, Target::Preview).unwrap_err();
    assert!(err.contains("props()"), "{err}");
}

const EXISTING: &str = "import { useState } from 'react';

interface CardProps {
  title: string;
}

export default function Card({ title }: CardProps) {
  const [count, setCount] = useState<number>(0);

  return (
    <div className=\"card\">
      <h1>{title}</h1>
    </div>
  );
}
";

#[test]
fn into_a_component_adds_props_members_and_elements() {
    let fs = MemFs::with(&[("src/Card.tsx", EXISTING)]);
    let expr = "props(size?:'sm'|'md'='md')+fn:inc():void{ setCount(count + 1) }+jsx>button[@click=inc]{+1}";
    let out = go(&fs, expr, false, into("src/Card.tsx")).unwrap();
    assert!(text(&out).contains("src/Card.tsx"), "{}", text(&out));
    let file = fs.get("src/Card.tsx").unwrap();
    assert!(
        file.contains("export default function Card({ title, size = 'md' }: CardProps) {"),
        "{file}"
    );
    assert!(
        file.contains("  title: string;\n  size?: 'sm' | 'md';\n}"),
        "{file}"
    );
    assert!(
        file.contains("useState<number>(0);\n\n  function inc(): void {\n    setCount(count + 1)\n  }\n\n  return ("),
        "{file}"
    );
    assert!(
        file.contains(
            "      <h1>{title}</h1>\n      <button onClick={inc}>+1</button>\n    </div>"
        ),
        "{file}"
    );
    assert!(fs.has_backup_of("src/Card.tsx"));
}

#[test]
fn into_a_component_merges_imports_and_refuses_clashes() {
    let fs = MemFs::with(&[("src/Card.tsx", EXISTING)]);
    go(
        &fs,
        "use:@/lib/fmt+jsx>p{{ fmt(title) }}",
        false,
        into("src/Card.tsx"),
    )
    .unwrap();
    let file = fs.get("src/Card.tsx").unwrap();
    assert!(file.contains("import fmt from '@/lib/fmt';"), "{file}");
    let err = go(&fs, "ref:count:int=1", false, into("src/Card.tsx")).unwrap_err();
    assert!(err.contains("already declared"), "{err}");
    let err = go(&fs, "props(title:str)", false, into("src/Card.tsx")).unwrap_err();
    assert!(err.contains("already a prop"), "{err}");
    // nothing else was written by the refused calls
    assert_eq!(fs.get("src/Card.tsx").unwrap(), file);
}

#[test]
fn dry_into_prints_the_new_lines_and_writes_nothing() {
    let fs = MemFs::with(&[("src/Card.tsx", EXISTING)]);
    let out = go(&fs, "jsx>p{Hi}", true, into("src/Card.tsx")).unwrap();
    assert!(text(&out).contains("<p>Hi</p>"), "{}", text(&out));
    assert_eq!(fs.get("src/Card.tsx").unwrap(), EXISTING);
}

#[test]
fn invalid_code_is_rejected_and_nothing_is_written() {
    let fs = MemFs::with(TS_PROJECT);
    let err = go(
        &fs,
        "react:Bad>fn:x():void{ if ( }+jsx>div",
        false,
        write("src/Bad.tsx"),
    )
    .unwrap_err();
    assert!(err.contains("syntax error"), "{err}");
    assert!(fs.get("src/Bad.tsx").is_none());
}

#[test]
fn names_and_sections_are_checked() {
    let fs = MemFs::with(TS_PROJECT);
    let err = go(&fs, "react:card>jsx>div", true, Target::Preview).unwrap_err();
    assert!(err.contains("capital"), "{err}");
    let err = go(&fs, "react:A>jsx>div", false, write("src/A.vue")).unwrap_err();
    assert!(err.contains("react:"), "{err}");
    let err = go(&fs, "jsx>div[v-model=x]", true, Target::Preview).unwrap_err();
    assert!(err.contains("v-model"), "{err}");
    let err = go(
        &fs,
        "fn:x(){}",
        false,
        Target::Replace {
            file: "src/Card.tsx".to_owned(),
            at: crate::generate::generator::ReplaceAt::Method {
                class: None,
                name: "x".to_owned(),
            },
            expect: None,
        },
    )
    .unwrap_err();
    assert!(err.contains("--replace"), "{err}");
}

#[test]
fn a_new_component_file_is_not_an_undo_step() {
    // like every single new file (PHP, Vue): nothing existing was changed, so there is no backup to restore
    let fs = MemFs::with(TS_PROJECT);
    go(&fs, "react:Tag>jsx>span{x}", false, write("src/Tag.tsx")).unwrap();
    assert!(fs.get("src/Tag.tsx").is_some());
    let clock = FixedClock(Cell::new(T0));
    let env = Env {
        root: Path::new(ROOT),
        fs: &fs,
        clock: &clock,
    };
    let err = undo(env, None).unwrap_err();
    assert!(err.message().contains("no backups"), "{err}");
    assert!(fs.get("src/Tag.tsx").is_some());
}

#[test]
fn a_written_component_is_never_overwritten() {
    let fs = MemFs::with(&[("src/Tag.tsx", "x")]);
    let err = go(&fs, "react:Tag>jsx>span", false, write("src/Tag.tsx")).unwrap_err();
    assert!(err.contains("already exists"), "{err}");
    assert_eq!(fs.get("src/Tag.tsx").unwrap(), "x");
}

#[test]
fn react_targets_and_heads_are_routed_to_this_generator() {
    let req = |expr: &str, target: Target| GenRequest {
        expr: expr.to_owned(),
        mode: Mode::Write,
        target,
    };
    assert!(wants_vue(&req("x", write("src/A.tsx"))));
    assert!(wants_vue(&req("x", into("src/A.jsx"))));
    assert!(wants_vue(&req("react:A>jsx>div", Target::Preview)));
    assert!(wants_vue(&req("hook:useA>ref:a", Target::Preview)));
    assert!(wants_vue(&req("jsx>div", Target::Preview)));
}
