use super::render::{action_name, model_path};
use super::scan::{self, Target, ViewRef, kebab};
use super::{Livewire, events};
use crate::i18n::Lang;

const COUNTER: &str = r#"<?php
namespace App\Livewire;

use Livewire\Attributes\Validate;
use Livewire\Component;

class Counter extends Component
{
    #[Validate('required|min:1')]
    public int $count = 0;

    public string $label = '';

    protected string $secret = 'x';

    public function increment(): void
    {
        $this->count++;
        $this->dispatch('counter-changed', count: $this->count);
    }

    public function render()
    {
        return view('livewire.counter');
    }
}
"#;

const COUNTER_VIEW: &str = r#"<div>
    <input wire:model="label">
    <input wire:model.live="cnt">
    <button wire:click="increment">+</button>
    <button wire:click="dec">-</button>
    <button wire:click="$refresh">r</button>
    <button wire:click="$set('count', 0)">s</button>
    <button wire:click="$toggle('open')">t</button>
    <button wire:click="$dispatch('reset')">d</button>
    <button wire:click="$parent.reload()">p</button>
    <button wire:click="{{ $action }}">dyn</button>
</div>
"#;

/// A project from in-memory files (no disk): every file is scanned as the walker would.
fn project(files: &[(&str, &str)]) -> Livewire {
    let mut out = Livewire::default();
    for (rel, text) in files {
        if rel.ends_with(".blade.php") {
            out.views.insert((*rel).to_owned(), scan::scan_view(text));
        } else if rel.starts_with("routes/") {
            out.routes.extend(scan::scan_routes(rel, text));
        } else if let Some(class) = scan::scan_class(rel, text) {
            out.classes.push(class);
        }
    }
    out.classes.sort_by(|a, b| a.rel.cmp(&b.rel));
    out.scope = files.iter().map(|(rel, _)| (*rel).to_owned()).collect();
    out
}

fn counter_project(class: &str, view: &str) -> Livewire {
    project(&[
        ("app/Livewire/Counter.php", class),
        ("resources/views/livewire/counter.blade.php", view),
    ])
}

#[test]
fn class_scan_reads_props_methods_view_rules_and_events() {
    let class = scan::scan_class("app/Livewire/Counter.php", COUNTER).unwrap();
    assert_eq!((class.name.as_str(), class.form), ("Counter", false));
    assert_eq!(class.fqn, "App\\Livewire\\Counter");
    assert_eq!(class.comp_name, "counter");
    let props: Vec<(&str, bool)> = class
        .props
        .iter()
        .map(|p| (p.name.as_str(), p.public))
        .collect();
    assert_eq!(props, [("count", true), ("label", true), ("secret", false)]);
    assert_eq!(class.props[0].ty.as_deref(), Some("int"));
    let methods: Vec<(&str, bool)> = class
        .methods
        .iter()
        .map(|m| (m.name.as_str(), m.public))
        .collect();
    assert_eq!(methods, [("increment", true), ("render", true)]);
    assert_eq!(class.view, ViewRef::Named("livewire.counter".to_owned()));
    assert_eq!(class.rules, [("count".to_owned(), true)]);
    assert!(class.complete);
    let sites: Vec<(&str, Option<&str>)> = class
        .dispatches
        .iter()
        .map(|s| (s.name.as_str(), s.method.as_deref()))
        .collect();
    assert_eq!(sites, [("counter-changed", Some("increment"))]);
}

#[test]
fn class_without_render_uses_the_convention_and_other_classes_are_ignored() {
    let text = "<?php\nuse Livewire\\Component;\nclass A extends Component {\n public $x;\n}\n";
    let class = scan::scan_class("app/Livewire/Admin/UserTable.php", text).unwrap();
    assert_eq!(class.view, ViewRef::Convention);
    assert_eq!(class.comp_name, "admin.user-table");
    // a controller that merely mentions Livewire, and a component of another library
    assert!(
        scan::scan_class(
            "app/Http/C.php",
            "<?php\nclass C extends Controller { /* Livewire */ }\n"
        )
        .is_none()
    );
    assert!(
        scan::scan_class(
            "app/X.php",
            "<?php\nclass X extends Component { }\n// Livewire\n"
        )
        .is_none()
    );
}

#[test]
fn listeners_rules_and_dynamic_names_are_read() {
    let text = r#"<?php
namespace App\Http\Livewire;
use Livewire\Attributes\On;
use Livewire\Component;
class Box extends Component
{
    protected $listeners = ['saved' => 'refresh', 'other', 'echo:orders,Shipped' => 'x'];
    protected $rules = ['name' => 'required|min:3', 'bio' => 'max:10'];
    #[On('post-created')]
    public function onPost() {}
    #[On('room.{id}')]
    public function dynamicRoom() {}
    public function getListeners() { return []; }
    public function save()
    {
        $this->validate(['email' => ['required', 'email']]);
        $this->dispatch('flash-' . $kind);
        $this->dispatchTo('other-box', 'ping');
        $this->emitUp('legacy');
    }
}
"#;
    let class = scan::scan_class("app/Http/Livewire/Box.php", text).unwrap();
    let listens: Vec<(&str, Option<&str>)> = class
        .listens
        .iter()
        .map(|s| (s.name.as_str(), s.method.as_deref()))
        .collect();
    assert_eq!(
        listens,
        [
            ("post-created", Some("onPost")),
            ("saved", Some("refresh")),
            ("other", Some("other"))
        ]
    );
    let dispatches: Vec<&str> = class.dispatches.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(dispatches, ["ping", "legacy"]);
    assert_eq!(
        class.rules,
        [
            ("name".to_owned(), true),
            ("bio".to_owned(), false),
            ("email".to_owned(), true)
        ]
    );
    let codes: Vec<&str> = class.dynamic.iter().map(|(code, _)| *code).collect();
    assert_eq!(codes.iter().filter(|c| **c == "lw-event").count(), 2);
    assert!(codes.contains(&"lw-listeners"));
}

#[test]
fn traits_make_the_member_list_incomplete() {
    let text = "<?php\nuse Livewire\\Component;\nclass P extends Component {\n use WithPagination;\n public $q;\n}\n";
    let class = scan::scan_class("app/Livewire/P.php", text).unwrap();
    assert!(!class.complete);
    let found = project(&[
        ("app/Livewire/P.php", text),
        (
            "resources/views/livewire/p.blade.php",
            "<a wire:click=\"nextPage\">n</a><i wire:model=\"page\"></i>",
        ),
    ]);
    // `nextPage` / `page` come from the trait: nothing is reported
    assert!(found.warnings(0, Lang::En).is_empty());
}

#[test]
fn view_scan_reads_uses_bindings_and_events() {
    let text = "<div>\n<livewire:admin.user-table />\n@livewire('post-list')\n@livewire(Counter::class)\n@livewire($name)\n<livewire:is :component=\"$x\" />\n{{-- <livewire:ghost /> --}}\n<input wire:model.live.debounce.300ms=\"q\">\n<button wire:click.prevent=\"go(1)\" wire:loading.class=\"x\">\n<b x-on:click=\"$dispatch('a')\" wire:click=\"$dispatch('b')\" onclick=\"$wire.dispatchTo('c', 'd')\">\n<script>Livewire.on('e', () => {}); $wire.dispatch(name);</script>\n</div>\n";
    let view = scan::scan_view(text);
    let uses: Vec<(&Target, u32)> = view.uses.iter().map(|u| (&u.target, u.line)).collect();
    assert_eq!(
        uses,
        [
            (&Target::Name("admin.user-table".to_owned()), 2),
            (&Target::Name("post-list".to_owned()), 3),
            (&Target::Class("Counter".to_owned()), 4)
        ]
    );
    assert_eq!(
        view.dynamic,
        [("lw-component", 6), ("lw-component", 5), ("lw-event", 11)]
    );
    let models: Vec<&str> = view.models.iter().map(|b| b.value.as_str()).collect();
    assert_eq!(models, ["q"]);
    let actions: Vec<(&str, &str)> = view
        .actions
        .iter()
        .map(|b| (b.attr.as_str(), b.value.as_str()))
        .collect();
    assert_eq!(
        actions,
        [("wire:click", "go(1)"), ("wire:click", "$dispatch('b')")]
    );
    let dispatches: Vec<&str> = view.dispatches.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(dispatches, ["a", "b", "d"]);
    assert_eq!(view.listens, [("e".to_owned(), 11)]);
}

#[test]
fn route_scan_resolves_the_class_through_the_use_lines() {
    let routes = scan::scan_routes(
        "routes/web.php",
        "<?php\nuse App\\Livewire\\Dashboard;\nuse App\\Livewire\\Box as B;\nRoute::get('/dash', Dashboard::class)->name('dash');\nRoute::post(\"/b\", B::class);\nRoute::get('/c', [Ctl::class, 'x']);\n// Route::get('/old', Old::class);\n",
    );
    let found: Vec<(&str, &str, &str, Option<&str>)> = routes
        .iter()
        .map(|r| {
            (
                r.verb.as_str(),
                r.uri.as_str(),
                r.class.as_str(),
                r.name.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            ("GET", "/dash", "App\\Livewire\\Dashboard", Some("dash")),
            ("POST", "/b", "App\\Livewire\\Box", None)
        ]
    );
}

#[test]
fn names_kebab_and_action_parsing() {
    assert_eq!(kebab("UserTable"), "user-table");
    assert_eq!(kebab("Counter2"), "counter2");
    assert_eq!(action_name("save"), Some("save"));
    assert_eq!(action_name("remove({{ $id }})"), Some("remove"));
    assert_eq!(action_name("go('a', 1)"), Some("go"));
    for magic in [
        "$refresh",
        "$set('a', 1)",
        "$toggle('a')",
        "$dispatch('x')",
        "$parent.save()",
        "count++",
        "{{ $name }}",
    ] {
        assert_eq!(action_name(magic), None, "{magic}");
    }
    assert_eq!(model_path("form.title"), Some(("form", Some("title"))));
    assert_eq!(model_path("items.0.name"), Some(("items", None)));
    assert_eq!(model_path("$parent.x"), None);
    assert_eq!(model_path("{{ $field }}"), None);
}

#[test]
fn contract_flags_a_missing_method_and_property_but_never_the_magic_ones() {
    let found = counter_project(COUNTER, COUNTER_VIEW);
    let texts: Vec<String> = found
        .warnings(0, Lang::En)
        .into_iter()
        .map(|w| format!("{} L{}", w.text, w.line))
        .collect();
    assert_eq!(
        texts,
        [
            "wire:click: no public method 'dec' in Counter L5",
            "wire:model: no public property 'cnt' in Counter L3"
        ]
    );
    // a protected property is no binding target
    let view = "<input wire:model=\"secret\">";
    let found = counter_project(COUNTER, view);
    assert_eq!(found.warnings(0, Lang::En).len(), 1);
}

#[test]
fn the_form_object_property_is_checked() {
    let form = "<?php\nuse Livewire\\Form;\nclass PostForm extends Form {\n #[\\Livewire\\Attributes\\Validate('required')]\n public string $title = '';\n}\n";
    let page = "<?php\nuse Livewire\\Component;\nclass Page extends Component {\n public PostForm $form;\n}\n";
    let found = project(&[
        ("app/Livewire/Forms/PostForm.php", form),
        ("app/Livewire/Page.php", page),
        (
            "resources/views/livewire/page.blade.php",
            "<i wire:model=\"form.title\"></i><i wire:model=\"form.nope\"></i>",
        ),
    ]);
    let page_index = found.components().next().unwrap();
    let texts: Vec<String> = found
        .warnings(page_index, Lang::En)
        .into_iter()
        .map(|w| w.text)
        .collect();
    assert_eq!(texts, ["wire:model: no public property 'nope' in PostForm"]);
    assert_eq!(found.rules_of(page_index), ["form.title*"]);
}

#[test]
fn uses_resolve_by_name_class_and_route() {
    let found = project(&[
        (
            "app/Livewire/Admin/UserTable.php",
            "<?php\nnamespace App\\Livewire\\Admin;\nuse Livewire\\Component;\nclass UserTable extends Component {}\n",
        ),
        (
            "routes/web.php",
            "<?php\nuse App\\Livewire\\Admin\\UserTable;\nRoute::get('/users', UserTable::class)->name('users');\n",
        ),
        (
            "resources/views/a.blade.php",
            "<livewire:admin.user-table />\n@livewire('admin.user-table')\n<livewire:admin.missing />\n",
        ),
    ]);
    let uses = found.uses_of(0);
    assert_eq!(
        uses,
        [
            ("resources/views/a.blade.php".to_owned(), 1),
            ("resources/views/a.blade.php".to_owned(), 2),
            ("routes/web.php".to_owned(), 3)
        ]
    );
    assert_eq!(found.routes_of(0)[0].uri, "/users");
}

#[test]
fn the_view_lists_blocks_findings_and_the_footer() {
    let found = counter_project(COUNTER, COUNTER_VIEW);
    let lines = found.lines(Lang::En);
    assert_eq!(lines[0], "livewire: 1 components, 0 uses, 2 warnings");
    assert_eq!(
        lines[1],
        "Counter  app/Livewire/Counter.php → livewire/counter.blade.php · used: 0"
    );
    assert_eq!(lines[2], "  props: count, label · actions: increment");
    assert_eq!(lines[3], "  events: dispatches counter-changed, reset");
    assert_eq!(lines[4], "  rules: count*");
    assert!(lines[5].starts_with("  ⚠ wire:click: no public method 'dec'"));
    assert!(lines.iter().any(|l| l.contains("wire:model")));
    // the Blade-built action name and the magic actions are never reported
    assert!(
        !lines
            .iter()
            .any(|l| l.contains("$action") || l.contains("refresh"))
    );
}

#[test]
fn the_header_of_a_class_and_of_its_template() {
    let found = project(&[
        ("app/Livewire/Counter.php", COUNTER),
        ("resources/views/livewire/counter.blade.php", COUNTER_VIEW),
        ("resources/views/home.blade.php", "<livewire:counter />\n"),
    ]);
    let class = found.header("app/Livewire/Counter.php", Lang::En);
    assert_eq!(
        class,
        [
            "view: resources/views/livewire/counter.blade.php",
            "used in: resources/views/home.blade.php L1"
        ]
    );
    let view = found.header("resources/views/livewire/counter.blade.php", Lang::En);
    assert_eq!(view[0], "livewire: Counter (app/Livewire/Counter.php)");
    assert_eq!(view[1], "calls: increment L4");
    assert_eq!(view[2], "binds: label, cnt");
    assert!(view[3].starts_with("⚠ wire:click: no public method 'dec'"));
    // a file that belongs to no component has no header
    assert!(
        found
            .header("resources/views/home.blade.php", Lang::En)
            .is_empty()
    );
}

#[test]
fn a_missing_named_view_is_flagged_only_when_the_project_has_views() {
    let class = "<?php\nuse Livewire\\Component;\nclass A extends Component {\n public function render() { return view('livewire.gone'); }\n}\n";
    let alone = project(&[("app/Livewire/A.php", class)]);
    assert!(alone.header("app/Livewire/A.php", Lang::En).is_empty());
    let with_views = project(&[
        ("app/Livewire/A.php", class),
        ("resources/views/x.blade.php", ""),
    ]);
    assert_eq!(
        with_views.header("app/Livewire/A.php", Lang::En),
        ["⚠ view 'livewire.gone' not found"]
    );
}

#[test]
fn trace_chain_of_a_component() {
    let found = project(&[
        ("app/Livewire/Counter.php", COUNTER),
        ("resources/views/livewire/counter.blade.php", COUNTER_VIEW),
        (
            "routes/web.php",
            "<?php\nuse App\\Livewire\\Counter;\nRoute::get('/counter', Counter::class)->name('counter');\n",
        ),
    ]);
    let models = |rel: &str| {
        assert_eq!(rel, "app/Livewire/Counter.php");
        vec!["Post(posts)".to_owned()]
    };
    for target in ["livewire:counter", "Counter", "app/Livewire/Counter.php"] {
        let lines = found.trace_lines(target, Lang::En, &models).unwrap();
        assert_eq!(
            lines[0], "livewire Counter  app/Livewire/Counter.php:7",
            "{target}"
        );
        assert_eq!(
            lines[1],
            "route GET /counter  name=counter  routes/web.php:3"
        );
        assert_eq!(lines[2], "view resources/views/livewire/counter.blade.php");
        assert!(lines.contains(&"used in 1: routes/web.php L3".to_owned()));
        assert!(lines.contains(&"props count, label".to_owned()));
        assert!(lines.contains(&"actions increment".to_owned()));
        assert!(lines.contains(&"rules count*".to_owned()));
        assert!(lines.contains(&"models Post(posts)".to_owned()));
    }
    assert!(
        found
            .trace_lines("livewire:nope", Lang::En, &models)
            .is_none()
    );
    assert_eq!(found.trace_candidates(), ["livewire:counter"]);
}

#[test]
fn event_calls_carry_the_shared_key() {
    let text = "<?php\nuse Livewire\\Attributes\\On;\nuse Livewire\\Component;\nclass L extends Component {\n #[On('refresh-list')]\n public function reload() {}\n public function save() { $this->dispatch('refresh-list'); }\n}\n";
    let calls = events::php_event_calls("app/Livewire/L.php", text);
    let found: Vec<(&str, Option<&str>, u32)> = calls
        .iter()
        .map(|c| (c.target.as_str(), c.caller.as_deref(), c.line))
        .collect();
    assert_eq!(
        found,
        [
            ("livewire::event:refresh-list", Some("save"), 7),
            ("livewire::event:refresh-list", Some("reload"), 6)
        ]
    );
    assert!(calls.iter().all(|c| c.category == "framework_use"
        && c.resolved.as_deref() == Some("livewire::event:refresh-list")));
    assert!(
        events::php_event_calls(
            "app/X.php",
            "<?php\nclass X { function f() { $this->dispatch('a'); } }\n"
        )
        .is_empty()
    );
}
