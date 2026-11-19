//! The preset in an Inertia + React project: `.tsx` / `.jsx` pages with the project's own components, `useForm`
//! from `@inertiajs/react`, translation only where existing pages translate.

use std::cell::Cell;

use super::tests::{COMPOSER, MIGRATION, MODEL, T0, WEB, env, run};
use crate::generate::generator::undo;
use crate::generate::testing::{FixedClock, MemFs};

const TEXT_INPUT: &str = "import { ChangeEvent } from 'react';

export default function TextInput({ value, onChange, id, type = 'text', className = '' }: {
    value: string;
    onChange: (e: ChangeEvent<HTMLInputElement>) => void;
    id?: string;
    type?: string;
    className?: string;
}) {
    return <input id={id} type={type} className={className} value={value} onChange={onChange} />;
}
";

const INPUT_LABEL: &str = "export default function InputLabel({ value, htmlFor, className = '' }: {
    value?: string;
    htmlFor?: string;
    className?: string;
}) {
    return <label htmlFor={htmlFor} className={className}>{value}</label>;
}
";

const INPUT_ERROR: &str = "export default function InputError({ message, className = '' }: { message?: string; className?: string }) {
    return message ? <p className={className}>{message}</p> : null;
}
";

const PRIMARY_BUTTON: &str = "import { ButtonHTMLAttributes } from 'react';

export default function PrimaryButton({ className = '', disabled, children, ...props }: ButtonHTMLAttributes<HTMLButtonElement>) {
    return <button {...props} className={className} disabled={disabled}>{children}</button>;
}
";

const LAYOUT: &str = "import { PropsWithChildren, ReactNode } from 'react';

export default function Authenticated({ header, children }: PropsWithChildren<{ header?: ReactNode }>) {
    return <div>{header}<main>{children}</main></div>;
}
";

/// A Laravel + Inertia + React (TypeScript) project with Breeze-like components, a model and a migration.
fn project() -> MemFs {
    MemFs::with(&[
        ("composer.json", COMPOSER),
        (
            "package.json",
            r#"{"dependencies":{"@inertiajs/react":"^1.0.0","react":"^18.0.0"},"devDependencies":{"typescript":"^5.0.0"}}"#,
        ),
        ("tsconfig.json", "{}"),
        ("routes/web.php", WEB),
        ("app/Models/Fee.php", MODEL),
        (
            "database/migrations/2024_01_01_000000_create_fees_table.php",
            MIGRATION,
        ),
        ("resources/js/Components/TextInput.tsx", TEXT_INPUT),
        ("resources/js/Components/InputLabel.tsx", INPUT_LABEL),
        ("resources/js/Components/InputError.tsx", INPUT_ERROR),
        ("resources/js/Components/PrimaryButton.tsx", PRIMARY_BUTTON),
        ("resources/js/Layouts/AuthenticatedLayout.tsx", LAYOUT),
    ])
}

const ALL: &str =
    "laravel:Fee>create(name:str,amount:int,paid?:bool,note?:text)+edit+index+show+destroy";

#[test]
fn an_inertia_react_project_gets_four_tsx_pages_and_no_vue() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    let out = run(&fs, &clock, ALL, false).unwrap();
    for page in ["Index", "Create", "Show", "Edit"] {
        assert!(
            fs.get(&format!("resources/js/Pages/Fees/{page}.tsx"))
                .is_some(),
            "{page}: {out}"
        );
    }
    assert!(
        fs.names_under("resources/js/Pages")
            .iter()
            .all(|p| p.ends_with(".tsx")),
        "{:?}",
        fs.names_under("resources/js/Pages")
    );
    assert!(out.contains("✓ form ↔ rules"), "{out}");
    // the PHP side is the same as in a Vue project
    assert!(fs.get("app/Http/Requests/StoreFeeRequest.php").is_some());
    assert!(fs.get("app/Http/Controllers/FeeController.php").is_some());
}

#[test]
fn the_create_page_uses_use_form_and_the_projects_components() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    run(&fs, &clock, ALL, false).unwrap();
    let page = fs.get("resources/js/Pages/Fees/Create.tsx").unwrap();
    assert!(
        page.contains("import { Head, useForm } from '@inertiajs/react';"),
        "{page}"
    );
    assert!(
        page.contains("import { type FormEvent } from 'react';"),
        "{page}"
    );
    assert!(
        page.contains("import AuthenticatedLayout from '@/Layouts/AuthenticatedLayout';"),
        "{page}"
    );
    assert!(
        page.contains("import InputLabel from '@/Components/InputLabel';"),
        "{page}"
    );
    assert!(
        page.contains("import TextInput from '@/Components/TextInput';"),
        "{page}"
    );
    assert!(
        page.contains("import InputError from '@/Components/InputError';"),
        "{page}"
    );
    assert!(
        page.contains("import PrimaryButton from '@/Components/PrimaryButton';"),
        "{page}"
    );
    assert!(
        page.contains("export default function Create() {"),
        "{page}"
    );
    assert!(page.contains("const form = useForm({"), "{page}");
    assert!(page.contains("function submit(e: FormEvent) {"), "{page}");
    assert!(page.contains("e.preventDefault();"), "{page}");
    assert!(page.contains("form.post(route('fees.store'));"), "{page}");
    assert!(page.contains("onSubmit={submit}"), "{page}");
    assert!(
        page.contains("<InputLabel htmlFor=\"name\" value=\"Name\" />"),
        "{page}"
    );
    assert!(page.contains("value={form.data.name}"), "{page}");
    assert!(
        page.contains("onChange={(e) => form.setData('name', e.target.value)}"),
        "{page}"
    );
    assert!(page.contains("value={form.data.amount ?? ''}"), "{page}");
    assert!(page.contains("Number(e.target.value)"), "{page}");
    assert!(page.contains("checked={form.data.paid}"), "{page}");
    assert!(
        page.contains("<InputError className=\"mt-2\" message={form.errors.name} />"),
        "{page}"
    );
    assert!(
        page.contains("<PrimaryButton disabled={form.processing}>Save</PrimaryButton>"),
        "{page}"
    );
    assert!(page.contains("<Head title=\"Create Fee\" />"), "{page}");
    assert!(
        page.contains("header={<h2 className=\"text-xl font-semibold leading-tight text-gray-800\">Create Fee</h2>}"),
        "{page}"
    );
    assert!(
        !page.contains("v-model") && !page.contains("$t(") && !page.contains("props."),
        "{page}"
    );
}

#[test]
fn edit_and_index_pages_read_their_props_and_link_with_route() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    run(&fs, &clock, ALL, false).unwrap();
    let edit = fs.get("resources/js/Pages/Fees/Edit.tsx").unwrap();
    assert!(edit.contains("interface EditProps {"), "{edit}");
    assert!(
        edit.contains("export default function Edit({ fee }: EditProps) {"),
        "{edit}"
    );
    assert!(edit.contains("name: fee.name,"), "{edit}");
    assert!(
        edit.contains("form.put(route('fees.update', fee.id));"),
        "{edit}"
    );

    let index = fs.get("resources/js/Pages/Fees/Index.tsx").unwrap();
    assert!(
        index.contains("import { Head, Link, router } from '@inertiajs/react';"),
        "{index}"
    );
    assert!(
        index.contains("export default function Index({ fees }: IndexProps) {"),
        "{index}"
    );
    assert!(index.contains("{fees.data.map((item) => ("), "{index}");
    assert!(index.contains("<tr key={item.id}>"), "{index}");
    assert!(index.contains("href={route('fees.create')}"), "{index}");
    assert!(
        index.contains("onClick={() => destroy(item.id)}"),
        "{index}"
    );
    assert!(
        index.contains("router.delete(route('fees.destroy', id));"),
        "{index}"
    );

    let show = fs.get("resources/js/Pages/Fees/Show.tsx").unwrap();
    assert!(show.contains("<dd"), "{show}");
}

/// The same project in plain JavaScript: `.jsx` components, no `tsconfig.json`, no `typescript`.
fn js_project() -> MemFs {
    MemFs::with(&[
        ("composer.json", COMPOSER),
        (
            "package.json",
            r#"{"dependencies":{"@inertiajs/react":"^1.0.0","react":"^18.0.0"}}"#,
        ),
        ("routes/web.php", WEB),
        ("app/Models/Fee.php", MODEL),
        (
            "resources/js/Components/TextInput.jsx",
            "export default function TextInput({ value, onChange, id }) {\n    return <input id={id} value={value} onChange={onChange} />;\n}\n",
        ),
        (
            "resources/js/Components/InputLabel.jsx",
            "export default function InputLabel({ value, htmlFor }) {\n    return <label htmlFor={htmlFor}>{value}</label>;\n}\n",
        ),
        (
            "resources/js/Components/InputError.jsx",
            "export default function InputError({ message }) {\n    return <p>{message}</p>;\n}\n",
        ),
        (
            "resources/js/Components/PrimaryButton.jsx",
            "export default function PrimaryButton({ children }) {\n    return <button>{children}</button>;\n}\n",
        ),
    ])
}

#[test]
fn a_javascript_project_gets_jsx_pages_without_types() {
    let fs = js_project();
    let clock = FixedClock(Cell::new(T0));
    run(&fs, &clock, "laravel:Fee>create(name:str)+edit", false).unwrap();
    assert!(fs.get("resources/js/Pages/Fees/Create.tsx").is_none());
    let page = fs.get("resources/js/Pages/Fees/Create.jsx").unwrap();
    assert!(page.contains("function submit(e) {"), "{page}");
    assert!(
        !page.contains("FormEvent") && !page.contains("interface"),
        "{page}"
    );
    // the sampled project is four-space indented
    assert!(page.contains("\n    const form = useForm({"), "{page}");
    let edit = fs.get("resources/js/Pages/Fees/Edit.jsx").unwrap();
    assert!(
        edit.contains("export default function Edit({ fee }) {"),
        "{edit}"
    );
}

const WELCOME: &str = "import { useTranslation } from 'react-i18next';

export default function Welcome() {
    const { t } = useTranslation();

    return <h1>{t('welcome.title')}</h1>;
}
";

#[test]
fn translation_is_used_only_when_existing_pages_translate() {
    // react-i18next is installed but no page translates: plain text
    let fs = project();
    fs.put(
        "package.json",
        r#"{"dependencies":{"@inertiajs/react":"^1.0.0","react-i18next":"^14.0.0"},"devDependencies":{"typescript":"^5.0.0"}}"#,
    );
    let clock = FixedClock(Cell::new(T0));
    let out = run(&fs, &clock, "laravel:Fee>create(name:str)", false).unwrap();
    let page = fs.get("resources/js/Pages/Fees/Create.tsx").unwrap();
    assert!(!page.contains("t('"), "{page}");
    assert!(page.contains(">Save</PrimaryButton>"), "{page}");
    assert!(!out.contains("i18n:"), "{out}");

    // an existing page translates: keys with the project's hook, the missing keys listed, no language file written
    let fs = project();
    fs.put("resources/js/Pages/Welcome.tsx", WELCOME);
    let out = run(&fs, &clock, "laravel:Fee>create(name:str)", false).unwrap();
    let page = fs.get("resources/js/Pages/Fees/Create.tsx").unwrap();
    assert!(
        page.contains("import { useTranslation } from 'react-i18next';"),
        "{page}"
    );
    assert!(page.contains("const { t } = useTranslation();"), "{page}");
    assert!(page.contains("{t('fees.save')}"), "{page}");
    assert!(page.contains("value={t('fees.fields.name')}"), "{page}");
    assert!(page.contains("title={t('fees.create_title')}"), "{page}");
    assert!(out.contains("i18n: no lang files found"), "{out}");
    assert_eq!(fs.names_under("lang").len(), 0);

    // gen.toml switches it off
    let fs = project();
    fs.put("resources/js/Pages/Welcome.tsx", WELCOME);
    fs.put(".callgraph/gen.toml", "[i18n]\nenabled = false\n");
    run(&fs, &clock, "laravel:Fee>create(name:str)", false).unwrap();
    let page = fs.get("resources/js/Pages/Fees/Create.tsx").unwrap();
    assert!(
        !page.contains("t('") && !page.contains("useTranslation"),
        "{page}"
    );
}

#[test]
fn gen_toml_overrides_the_component_heuristic() {
    let fs = project();
    fs.put(
        "resources/js/Components/BaseInput.tsx",
        "export function BaseInput({ value, onChange }: { value: string; onChange: () => void }) {\n    return <input value={value} onChange={onChange} />;\n}\n",
    );
    fs.put(
        ".callgraph/gen.toml",
        "[components]\ninput = \"BaseInput\"\n",
    );
    let clock = FixedClock(Cell::new(T0));
    let out = run(&fs, &clock, "laravel:Fee>create(name:str)", false).unwrap();
    let page = fs.get("resources/js/Pages/Fees/Create.tsx").unwrap();
    // a named export is imported by name
    assert!(
        page.contains("import { BaseInput } from '@/Components/BaseInput';"),
        "{page}"
    );
    assert!(page.contains("<BaseInput"), "{page}");
    assert!(!page.contains("<TextInput"), "{page}");
    assert!(
        out.contains("components (gen.toml): input=BaseInput"),
        "{out}"
    );
}

#[test]
fn components_are_found_by_name_and_props_when_the_usual_names_are_missing() {
    let fs = MemFs::with(&[
        ("composer.json", COMPOSER),
        (
            "package.json",
            r#"{"dependencies":{"@inertiajs/react":"^1.0.0"},"devDependencies":{"typescript":"^5"}}"#,
        ),
        ("routes/web.php", WEB),
        ("app/Models/Fee.php", MODEL),
        (
            "resources/js/Components/FieldInput.tsx",
            "export default function FieldInput({ value, onChange }: { value: string; onChange: () => void }) {\n    return <input value={value} onChange={onChange} />;\n}\n",
        ),
        (
            "resources/js/Components/FieldError.tsx",
            "export default function FieldError({ error }: { error?: string }) {\n    return <p>{error}</p>;\n}\n",
        ),
        (
            "resources/js/Components/FieldLabel.tsx",
            "export default function FieldLabel({ label }: { label?: string }) {\n    return <label>{label}</label>;\n}\n",
        ),
        (
            "resources/js/Components/SubmitButton.tsx",
            "export default function SubmitButton({ children }: { children?: string }) {\n    return <button type=\"submit\">{children}</button>;\n}\n",
        ),
        ("resources/js/Layouts/MainLayout.tsx", LAYOUT),
        ("resources/js/Layouts/GuestLayout.tsx", LAYOUT),
    ]);
    let clock = FixedClock(Cell::new(T0));
    let out = run(&fs, &clock, "laravel:Fee>create(name:str)", false).unwrap();
    let page = fs.get("resources/js/Pages/Fees/Create.tsx").unwrap();
    assert!(page.contains("<FieldInput"), "{page}");
    assert!(page.contains("<FieldLabel"), "{page}");
    assert!(page.contains("label=\"Name\""), "{page}");
    assert!(page.contains("<FieldError"), "{page}");
    assert!(page.contains("error={form.errors.name}"), "{page}");
    assert!(page.contains("<SubmitButton"), "{page}");
    assert!(page.contains("<MainLayout"), "{page}");
    assert!(!page.contains("GuestLayout"), "{page}");
    assert!(out.contains("components guessed:"), "{out}");
}

#[test]
fn dry_prints_the_pages_and_writes_nothing() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    let before = fs.snapshot();
    let out = run(&fs, &clock, "laravel:Fee>create(name:str)+index", true).unwrap();
    assert!(
        out.contains("== resources/js/Pages/Fees/Create.tsx"),
        "{out}"
    );
    assert!(
        out.contains("== resources/js/Pages/Fees/Index.tsx"),
        "{out}"
    );
    assert!(out.contains("useForm({"), "{out}");
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn one_undo_removes_the_react_pages_with_the_rest() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    run(&fs, &clock, ALL, false).unwrap();
    assert!(fs.get("resources/js/Pages/Fees/Create.tsx").is_some());
    undo(env(&fs, &clock), None).unwrap();
    assert_eq!(fs.get("routes/web.php").unwrap(), WEB);
    assert!(fs.get("resources/js/Pages/Fees/Create.tsx").is_none());
    assert!(fs.get("resources/js/Pages/Fees/Edit.tsx").is_none());
    assert!(fs.get("app/Http/Requests/StoreFeeRequest.php").is_none());
}

#[test]
fn a_project_with_both_inertia_packages_keeps_the_vue_pages() {
    let fs = project();
    fs.put(
        "package.json",
        r#"{"dependencies":{"@inertiajs/react":"^1.0.0","@inertiajs/vue3":"^1.0.0"}}"#,
    );
    let clock = FixedClock(Cell::new(T0));
    run(&fs, &clock, "laravel:Fee>create(name:str)", false).unwrap();
    assert!(fs.get("resources/js/Pages/Fees/Create.vue").is_some());
    assert!(fs.get("resources/js/Pages/Fees/Create.tsx").is_none());
}
