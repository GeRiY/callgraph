//! Az űrlap-szerződés tesztjei memóriabeli fájlrendszeren: a `useForm` kulcsok kiolvasása (beágyazott értékek,
//! megjegyzések, több űrlap egy fájlban), a küldés célja (`route('név')`, literál URL `{param}`-mal), a validáció
//! (FormRequest `use`-on át, inline `validate`, `confirmed`, explicit olvasás, `sometimes` / `nullable`), a
//! szabály nélküli akció, a kimenetek szövege (EN / HU) és a `forms` nézet sorai.

use std::path::{Path, PathBuf};

use super::Laravel;
use super::forms::{Form, extract};
use super::rules;
use super::tests::{MemFiles, call, graph, root};
use crate::i18n::Lang;
use crate::model::{Call, FileGraph};

const USE_FORM: &str = "@inertiajs/vue3::useForm";

const IMPORTS: &[(&str, &str)] = &[
    (
        "RegisterController",
        "App\\Http\\Controllers\\RegisterController",
    ),
    ("UserController", "App\\Http\\Controllers\\UserController"),
    (
        "PasswordController",
        "App\\Http\\Controllers\\PasswordController",
    ),
    ("FooController", "App\\Http\\Controllers\\FooController"),
];

/// A (1-től számozott) sor, amelyik először tartalmazza a mintát.
fn line_of(text: &str, needle: &str) -> u32 {
    let index = text
        .lines()
        .position(|line| line.contains(needle))
        .expect("needle in text");
    u32::try_from(index).unwrap() + 1
}

fn use_form(text: &str, decl: &str) -> Call {
    call(
        "func_call",
        "useForm",
        Some(USE_FORM),
        "inertia",
        line_of(text, decl),
    )
}

/// A küldő hívás (`form.post(..)`) a `submit` függvényben, a `Page` konténerben.
fn submit(text: &str, needle: &str, target: &str) -> Call {
    let verb = target.rsplit('.').next().unwrap();
    let mut call = call(
        "method_call",
        target,
        Some(&format!("{USE_FORM}().{verb}")),
        "inertia",
        line_of(text, needle),
    );
    call.caller = Some("submit".into());
    call.container = Some("Page".into());
    call
}

/// A route-fájl, a PHP-fájlok és a Vue-fájlok (rel, forrás, hívások) szerint feltöltött Laravel-kötés.
fn project(routes: &str, files: &[(&str, &str)], vues: &[(&str, &str, Vec<Call>)]) -> Laravel {
    let mut all: Vec<(&str, &str)> = files.to_vec();
    all.push(("routes/web.php", routes));
    all.extend(vues.iter().map(|(rel, text, _)| (*rel, *text)));
    let mem = MemFiles::new(&all);
    let web = graph("/p/routes/web.php", None, IMPORTS, Vec::new());
    let web_path = root().join("routes/web.php");
    let vue_graphs: Vec<(PathBuf, FileGraph)> = vues
        .iter()
        .map(|(rel, _, calls)| {
            (
                root().join(rel),
                graph(&format!("/p/{rel}"), Some(*rel), &[], calls.clone()),
            )
        })
        .collect();
    let mut graphs: Vec<(&Path, &FileGraph)> = vec![(web_path.as_path(), &web)];
    graphs.extend(
        vue_graphs
            .iter()
            .map(|(path, graph)| (path.as_path(), graph)),
    );
    let mut laravel = Laravel::new(&root());
    laravel.scan(graphs.iter().copied(), &mem, false);
    laravel.scan_forms(graphs.iter().copied(), &mem);
    laravel
}

const REGISTER_PAGE: &str = "resources/js/Pages/Auth/Register.vue";

const REGISTER_VUE: &str = r"<script setup>
import { useForm } from '@inertiajs/vue3';

const form = useForm({
    name: '', // full name
    email: '',
    password: '',
    password_confirmation: '',
    remember: false,
    extra: { a: 1, list: [1, 2], 'x,y': ',' },
    items: [{ name: '' }],
});

const submit = () => {
    form.post(route('register'), {
        onFinish: () => form.reset('password'),
    });
};
</script>
";

const REGISTER_ROUTES: &str = "<?php
Route::get('register', [RegisterController::class, 'create'])->name('register');
Route::post('register', [RegisterController::class, 'store']);
";

const REGISTER_CONTROLLER: &str = r"<?php
namespace App\Http\Controllers;

use App\Http\Requests\RegisterRequest;

class RegisterController extends Controller
{
    public function create() {}

    public function store(RegisterRequest $request)
    {
        $remember = $request->boolean('remember');
        return redirect('/');
    }
}
";

const REGISTER_REQUEST: &str = r"<?php
namespace App\Http\Requests;

use Illuminate\Foundation\Http\FormRequest;
use Illuminate\Validation\Rule;

class RegisterRequest extends FormRequest
{
    public function rules(): array
    {
        return [
            'name' => 'required|string|max:255',
            'email' => ['required', 'email', Rule::unique('users')],
            'password' => ['required', 'confirmed'],
            'nickname' => 'nullable|string',
            'terms' => 'required|accepted',
            'items.*.name' => 'required',
            'bio' => 'sometimes|required',
            'plan' => 'required_if:paid,1',
        ];
    }
}
";

fn register_project() -> Laravel {
    project(
        REGISTER_ROUTES,
        &[
            (
                "app/Http/Controllers/RegisterController.php",
                REGISTER_CONTROLLER,
            ),
            ("app/Http/Requests/RegisterRequest.php", REGISTER_REQUEST),
        ],
        &[(
            REGISTER_PAGE,
            REGISTER_VUE,
            vec![
                use_form(REGISTER_VUE, "const form = useForm"),
                submit(REGISTER_VUE, "form.post(route", "form.post"),
            ],
        )],
    )
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn use_form_keys_cover_nested_values_comments_and_the_remember_key() {
    let text = "const a = useForm('Login', {\n  email: '', // a, b\n  'e-mail': '',\n  deep: { x: [1, { y: 2 }] },\n  /* c: 1, */ last,\n});\nconst submit = () => a.post('/x');\n";
    let calls = vec![
        use_form(text, "const a"),
        submit(text, "a.post('/x')", "a.post"),
    ];
    let sites = extract("p.vue", text, &calls);
    assert_eq!(sites.len(), 1);
    assert_eq!(
        sites[0].fields,
        strings(&["email", "e-mail", "deep", "last"])
    );
    assert_eq!(sites[0].arg, "'/x'");
    assert_eq!(sites[0].verb, "POST");
}

#[test]
fn a_non_literal_or_spread_form_is_skipped() {
    let text = "const a = useForm(data);\nconst b = useForm({ ...base, x: 1 });\nconst s = () => { a.post('/x'); b.post('/y'); };\n";
    let calls = vec![
        use_form(text, "const a"),
        use_form(text, "const b"),
        submit(text, "a.post", "a.post"),
        submit(text, "b.post", "b.post"),
    ];
    assert!(extract("p.vue", text, &calls).is_empty());
}

/// A template-ben (`@submit.prevent="form.patch(..)"`) a küldésnek nincs `resolved`-je, és a template a script előtt
/// is állhat; a mező nélküli űrlap (`useForm({})`) kimarad.
#[test]
fn a_template_submit_binds_by_name_and_an_empty_form_is_skipped() {
    let text = "<form @submit.prevent=\"form.patch('/p')\"></form>\n<button @click=\"empty.post('/e')\"></button>\nconst form = useForm({ name: '' });\nconst empty = useForm({});\n";
    let mut patch = submit(text, "form.patch", "form.patch");
    patch.resolved = None;
    let mut post = submit(text, "empty.post", "empty.post");
    post.resolved = None;
    let calls = vec![
        patch,
        post,
        use_form(text, "const form"),
        use_form(text, "const empty"),
    ];
    let sites = extract("p.vue", text, &calls);
    assert_eq!(sites.len(), 1);
    assert_eq!(
        (sites[0].verb, sites[0].fields.clone(), sites[0].line),
        ("PATCH", strings(&["name"]), 1)
    );
}

#[test]
fn two_forms_in_one_file_keep_their_own_fields() {
    let text = "const form = useForm({ name: '' });\nconst passwordForm = useForm({ password: '' });\nconst a = () => form.put('/a');\nconst b = () => passwordForm.patch('/b');\n";
    let calls = vec![
        use_form(text, "const form"),
        use_form(text, "const passwordForm"),
        submit(text, "form.put('/a')", "form.put"),
        submit(text, "passwordForm.patch", "passwordForm.patch"),
    ];
    let sites = extract("p.vue", text, &calls);
    assert_eq!(sites.len(), 2);
    assert_eq!(
        (sites[0].verb, sites[0].fields.clone()),
        ("PUT", strings(&["name"]))
    );
    assert_eq!(
        (sites[1].verb, sites[1].fields.clone()),
        ("PATCH", strings(&["password"]))
    );
}

#[test]
fn a_form_request_via_import_confirmed_and_explicit_reads_make_the_contract() {
    let laravel = register_project();
    let form = &laravel.forms[0];
    assert_eq!(laravel.forms.len(), 1);
    assert_eq!(form.file, REGISTER_PAGE);
    assert_eq!(
        (form.verb.as_str(), form.uri.as_str()),
        ("POST", "/register")
    );
    assert_eq!(
        form.action(),
        "App\\Http\\Controllers\\RegisterController@store"
    );
    assert_eq!(
        form.fields,
        strings(&[
            "name",
            "email",
            "password",
            "password_confirmation",
            "remember",
            "extra",
            "items"
        ])
    );
    assert!(form.validated);
    assert_eq!(
        form.rules,
        strings(&[
            "bio", "email", "items", "name", "nickname", "password", "plan", "terms"
        ])
    );
    // `password_confirmation` (confirmed), `remember` (boolean), `items` (items.*.name) rendben;
    // a nullable / sometimes / required_if / nested kulcsot nem jelenti
    assert_eq!(form.unvalidated, strings(&["extra"]));
    assert_eq!(form.missing_required, strings(&["terms"]));
}

#[test]
fn the_outline_line_is_one_line_in_both_languages() {
    let laravel = register_project();
    let file = format!("/p/{REGISTER_PAGE}");
    assert_eq!(
        laravel.form_lines(&file, Some("Page"), "submit", Lang::En),
        vec![
            "form: POST /register · RegisterController@store · ⚠ unvalidated: extra · ⚠ required not sent: terms"
                .to_owned()
        ]
    );
    assert_eq!(
        laravel.form_lines(REGISTER_PAGE, Some("Page"), "submit", Lang::Hu),
        vec![
            "form: POST /register · RegisterController@store · ⚠ validálatlan: extra · ⚠ kötelező, de nem küldi: terms"
                .to_owned()
        ]
    );
    // másik függvény / konténer alatt nincs sor
    assert!(
        laravel
            .form_lines(&file, Some("Page"), "other", Lang::En)
            .is_empty()
    );
    assert!(
        laravel
            .form_lines(&file, None, "submit", Lang::En)
            .is_empty()
    );
}

#[test]
fn a_clean_form_ends_with_the_field_count() {
    let text =
        "const form = useForm({ name: '', _token: '' });\nconst s = () => form.post('/x');\n";
    let routes = "<?php\nRoute::post('/x', [FooController::class, 'store']);\n";
    let controller = r"<?php
namespace App\Http\Controllers;
class FooController {
    public function store(Request $request)
    {
        $request->validate(['name' => 'required']);
    }
}
";
    let laravel = project(
        routes,
        &[("app/Http/Controllers/FooController.php", controller)],
        &[(
            "resources/js/Pages/Foo.vue",
            text,
            vec![
                use_form(text, "const form"),
                submit(text, "form.post('/x')", "form.post"),
            ],
        )],
    );
    let lines =
        |lang| laravel.form_lines("resources/js/Pages/Foo.vue", Some("Page"), "submit", lang);
    assert_eq!(
        lines(Lang::En),
        vec!["form: POST /x · FooController@store · 2 fields ✓".to_owned()]
    );
    assert_eq!(
        lines(Lang::Hu),
        vec!["form: POST /x · FooController@store · 2 mező ✓".to_owned()]
    );
}

#[test]
fn a_literal_url_with_a_param_matches_and_inline_rules_are_read() {
    let text = "const form = useForm({ name: '', email: '' });\nconst passwordForm = useForm({ current_password: '', password: '' });\nconst a = () => form.put('/users/5');\nconst b = () => passwordForm.put(route('password.update'));\nconst c = () => form.delete(`/users/${id}`);\n";
    let routes = "<?php
Route::put('users/{user}', [UserController::class, 'update']);
Route::put('password', [PasswordController::class, 'update'])->name('password.update');
";
    let users = r"<?php
namespace App\Http\Controllers;
class UserController {
    public function update(Request $request, User $user)
    {
        // $request->validate(['commented' => 'required']);
        $request->validate(['name' => 'required|string']);
    }
}
";
    let password = r"<?php
namespace App\Http\Controllers;
use Illuminate\Validation\Rules\Password;
class PasswordController {
    public function update(Request $request)
    {
        $validated = $request->validateWithBag('updatePassword', [
            'current_password' => ['required', 'current_password'],
            'password' => ['required', Password::defaults(), 'confirmed'],
        ]);
    }
}
";
    let laravel = project(
        routes,
        &[
            ("app/Http/Controllers/UserController.php", users),
            ("app/Http/Controllers/PasswordController.php", password),
        ],
        &[(
            "resources/js/Pages/P.vue",
            text,
            vec![
                use_form(text, "const form"),
                use_form(text, "const passwordForm"),
                submit(text, "form.put('/users/5')", "form.put"),
                submit(text, "passwordForm.put(route", "passwordForm.put"),
                submit(text, "form.delete(`", "form.delete"),
            ],
        )],
    );
    // a template-literal cél kimarad: két űrlap-küldés marad
    assert_eq!(laravel.forms.len(), 2);
    let (users, password) = (&laravel.forms[0], &laravel.forms[1]);
    assert_eq!(
        (users.uri.as_str(), users.method.as_str()),
        ("/users/{user}", "update")
    );
    assert_eq!(users.unvalidated, strings(&["email"]));
    assert!(users.missing_required.is_empty());
    assert_eq!(users.rules, strings(&["name"]));
    assert!(password.unvalidated.is_empty() && password.missing_required.is_empty());
    assert_eq!(password.rules, strings(&["current_password", "password"]));
}

#[test]
fn an_action_without_any_rules_reports_no_validation() {
    let text = "const form = useForm({ a: '' });\nconst s = () => form.post('/x');\n";
    let routes = "<?php\nRoute::post('/x', [FooController::class, 'store']);\n";
    let controller = r"<?php
namespace App\Http\Controllers;
use Illuminate\Http\Request;
class FooController {
    public function store(Request $request)
    {
        Foo::create($request->all());
    }
}
";
    let laravel = project(
        routes,
        &[("app/Http/Controllers/FooController.php", controller)],
        &[(
            "resources/js/Pages/Foo.vue",
            text,
            vec![
                use_form(text, "const form"),
                submit(text, "form.post('/x')", "form.post"),
            ],
        )],
    );
    let form = &laravel.forms[0];
    assert!(!form.validated && form.unvalidated.is_empty() && form.missing_required.is_empty());
    assert_eq!(
        laravel.form_lines(
            "resources/js/Pages/Foo.vue",
            Some("Page"),
            "submit",
            Lang::En
        ),
        vec!["form: POST /x · FooController@store · no validation found".to_owned()]
    );
    assert_eq!(
        laravel.form_lines(
            "resources/js/Pages/Foo.vue",
            Some("Page"),
            "submit",
            Lang::Hu
        ),
        vec!["form: POST /x · FooController@store · nincs validáció".to_owned()]
    );
    // a nézetben a validáció nélküli űrlap is sor, és külön számolva
    let scope = vec!["resources/js/Pages/Foo.vue".to_owned()];
    assert_eq!(
        laravel.form_view_lines(&scope, Lang::En),
        vec![
            "⚠ resources/js/Pages/Foo.vue L2 → POST /x · FooController@store · no validation found"
                .to_owned(),
            "forms: 1 checked, 0 with issues, 1 without validation".to_owned(),
        ]
    );
}

#[test]
fn long_name_lists_are_capped_at_five() {
    let form = Form {
        file: "resources/js/Pages/X.vue".into(),
        line: 3,
        target: "form.post".into(),
        caller: Some("submit".into()),
        container: Some("Page".into()),
        verb: "POST".into(),
        uri: "/x".into(),
        class: "App\\Http\\Controllers\\FooController".into(),
        method: "store".into(),
        fields: strings(&["a", "b", "c", "d", "e", "f", "g"]),
        rules: Vec::new(),
        validated: true,
        unvalidated: strings(&["a", "b", "c", "d", "e", "f", "g"]),
        missing_required: Vec::new(),
    };
    let mut laravel = Laravel::new(&root());
    laravel.forms.push(form);
    assert_eq!(
        laravel.form_lines("resources/js/Pages/X.vue", Some("Page"), "submit", Lang::En),
        vec!["form: POST /x · FooController@store · ⚠ unvalidated: a, b, c, d, e, +2".to_owned()]
    );
}

#[test]
fn the_controller_action_lists_the_forms_that_submit_to_it() {
    let pages = ["A", "B", "C", "D"];
    let text = "const form = useForm({ a: '' });\nconst s = () => form.post('/x');\n";
    let routes = "<?php\nRoute::post('/x', [FooController::class, 'store']);\n";
    let controller = "<?php\nnamespace App\\Http\\Controllers;\nclass FooController {\n    public function store(Request $request)\n    {\n        $request->validate(['a' => 'required']);\n    }\n}\n";
    let rels: Vec<String> = pages
        .iter()
        .map(|p| format!("resources/js/Pages/{p}.vue"))
        .collect();
    let vues: Vec<(&str, &str, Vec<Call>)> = rels
        .iter()
        .map(|rel| {
            (
                rel.as_str(),
                text,
                vec![
                    use_form(text, "const form"),
                    submit(text, "form.post('/x')", "form.post"),
                ],
            )
        })
        .collect();
    let laravel = project(
        routes,
        &[("app/Http/Controllers/FooController.php", controller)],
        &vues,
    );
    assert_eq!(
        laravel
            .form_line(Some("App\\Http\\Controllers"), "FooController", "store")
            .as_deref(),
        Some("form: Pages/A.vue, Pages/B.vue, Pages/C.vue, +1")
    );
    assert_eq!(
        laravel.form_line(Some("App\\Http\\Controllers"), "FooController", "update"),
        None
    );
    // a json-hoz a küldő hívás űrlapja
    let calls = &vues[0].2;
    let found = laravel
        .form_of("/p/resources/js/Pages/A.vue", &calls[1])
        .map(Form::action);
    assert_eq!(
        found.as_deref(),
        Some("App\\Http\\Controllers\\FooController@store")
    );
    assert!(
        laravel
            .form_of("/p/resources/js/Pages/A.vue", &calls[0])
            .is_none()
    );
}

#[test]
fn the_forms_view_lists_the_issues_and_counts() {
    let laravel = register_project();
    let scope = vec![REGISTER_PAGE.to_owned()];
    assert_eq!(
        laravel.form_view_lines(&scope, Lang::En),
        vec![
            "⚠ resources/js/Pages/Auth/Register.vue L15 → POST /register · RegisterController@store · unvalidated: extra · required not sent: terms"
                .to_owned(),
            "forms: 1 checked, 1 with issues, 0 without validation".to_owned(),
        ]
    );
    let hu = laravel.form_view_lines(&scope, Lang::Hu);
    assert_eq!(
        hu[1],
        "űrlapok: 1 ellenőrizve, 1 problémás, 0 validáció nélkül"
    );
    assert!(hu[0].contains("validálatlan: extra · kötelező, de nem küldi: terms"));
    // hatókörön kívüli fájl / nincs űrlap: üres (a nézet `(no Inertia forms found)`-ot ír)
    assert!(
        laravel
            .form_view_lines(&["other.vue".to_owned()], Lang::En)
            .is_empty()
    );
    assert!(
        Laravel::new(&root())
            .form_view_lines(&scope, Lang::En)
            .is_empty()
    );
}

#[test]
fn rule_keys_flags_and_nested_keys() {
    let request = r"<?php
class R extends FormRequest {
    public function rules() {
        return [
            'a' => 'bail|required|email',
            'b' => ['nullable', 'string'],
            'c.*.d' => 'required',
            'c' => 'array',
            'e' => 'required_with:a|confirmed',
            Rule::something() => 'required',
        ];
    }
}";
    let v = rules::request_validation(request);
    assert!(v.found);
    let keys: Vec<&str> = v.rules.keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["a", "b", "c", "e"]);
    assert!(v.rules["a"].required);
    assert!(v.rules["b"].nullable && !v.rules["b"].required);
    // a `c.*.d` szabálya nem teszi kötelezővé a `c`-t
    assert!(!v.rules["c"].required);
    assert!(!v.rules["e"].required && v.rules["e"].confirmed);
}

#[test]
fn inline_validation_styles_and_explicit_reads() {
    let body = r"
        $a = $request->validate(['x' => 'required']);
        $this->validate($request, ['y' => 'required']);
        $v = Validator::make($request->all(), ['z' => 'sometimes|required']);
        $rules = ['w' => 'required'];
        request()->validate($rules);
        $r = $request->boolean('remember') || $request->has('opt.deep');
        $request->only(['p', 'q']);
        $name = $request->title;
        $request->user();
    ";
    let v = rules::body_validation(body);
    assert!(v.found);
    let keys: Vec<&str> = v.rules.keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["w", "x", "y", "z"]);
    assert!(v.rules["z"].sometimes);
    let reads: Vec<&str> = v.reads.iter().map(String::as_str).collect();
    assert_eq!(reads, vec!["opt", "p", "q", "remember", "title"]);
    assert!(!rules::body_validation("$x = 1;").found);
}
