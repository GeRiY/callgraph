//! A Blade-frontend egységtesztjei: valósághű sablonok (layout + oldal, partial, anonim komponens `@props`-szal,
//! űrlap `route()`-tal, `@foreach`, `__()`), és a kimenet: container, szakaszok, hívások, ágak, property-k.

use super::*;
use crate::model::{Call, FileGraph};

const PAGE: &str = "@extends('layouts.app')

@section('title', 'Users')

@section('content')
    <h1>{{ __('auth.title') }}</h1>
    <x-alert type=\"error\" :message=\"$errors->first()\" />
    @auth
        <p>{{ auth()->user()->name }}</p>
    @endauth
    <form method=\"POST\" action=\"{{ route('users.store') }}\">
        @csrf
        <input placeholder=\"@lang('auth.failed')\">
    </form>
    @foreach ($users as $user)
        <li>{{ $user->name }} {{ Str::limit($user->bio, 20) }}</li>
    @endforeach
@endsection

@push('scripts')
    <script src=\"{{ asset('js/users.js') }}\"></script>
@endpush
";

const LAYOUT: &str = "<html>
<head>
    <title>@yield('title')</title>
    @stack('scripts')
</head>
<body>
    @include('partials.nav', ['active' => 'users'])
    @yield('content')
</body>
</html>
";

const ALERT: &str = "@props(['type' => 'info', 'message'])
<div class=\"alert alert-{{ $type }}\">
    {{ $message }}
    {{ $slot }}
</div>
";

fn analyze(path: &str, text: &str) -> FileGraph {
    let file = SourceFile::new(path, text.as_bytes().to_vec(), None);
    BladeFrontend.analyze(&file).unwrap().graph
}

fn page() -> FileGraph {
    analyze("resources/views/admin/users/index.blade.php", PAGE)
}

fn find<'g>(graph: &'g FileGraph, target: &str, line: u32) -> &'g Call {
    graph
        .calls
        .iter()
        .find(|c| c.target == target && c.line == line)
        .unwrap_or_else(|| panic!("no call {target} at L{line}: {:#?}", graph.calls))
}

#[test]
fn container_is_the_dotted_view_name_and_extends_the_layout() {
    let graph = page();
    let view = &graph.containers[0];
    assert_eq!(view.kind, "view");
    assert_eq!(view.name, "admin.users.index");
    assert_eq!(view.extends, Some(vec!["layouts.app".to_owned()]));
}

#[test]
fn sections_and_pushes_are_methods_with_line_ranges() {
    let graph = page();
    let methods: Vec<String> = graph
        .methods
        .iter()
        .map(|m| format!("{} L{}-{}", m.name, m.line, m.end_line.unwrap()))
        .collect();
    assert_eq!(
        methods,
        [
            "template L1-22",
            "section:content L5-18",
            "push:scripts L20-22"
        ]
    );
    // az egysoros `@section('title', 'Users')` nem blokk
    assert!(graph.methods.iter().all(|m| m.name != "section:title"));
    assert!(
        graph
            .methods
            .iter()
            .all(|m| m.kind.as_deref() == Some(BLOCK_KIND))
    );
}

#[test]
fn view_use_resolves_to_files_and_components_to_templates() {
    let graph = page();
    let extends = find(&graph, "layouts.app", 1);
    assert_eq!(extends.category, "view_use");
    assert_eq!(extends.caller.as_deref(), Some("template"));
    assert_eq!(
        extends.resolved.as_deref(),
        Some("resources/views/layouts/app.blade.php")
    );
    let alert = find(&graph, "x-alert", 7);
    assert_eq!(alert.category, "component_use");
    assert_eq!(alert.caller.as_deref(), Some("section:content"));
    assert_eq!(
        alert.resolved.as_deref(),
        Some("resources/views/components/alert.blade.php")
    );
}

#[test]
fn php_expressions_in_echoes_and_bound_attributes_keep_their_lines() {
    let graph = page();
    assert_eq!(
        find(&graph, "first", 7).caller.as_deref(),
        Some("section:content")
    );
    assert_eq!(find(&graph, "auth", 9).line, 9);
    assert_eq!(find(&graph, "user", 9).line, 9);
    let limit = find(&graph, "Str::limit", 16);
    assert_eq!(limit.caller.as_deref(), Some("section:content"));
    let asset = find(&graph, "asset", 21);
    assert_eq!(asset.caller.as_deref(), Some("push:scripts"));
}

#[test]
fn route_and_translation_helpers_carry_the_names_for_the_cross_references() {
    let graph = page();
    let route = find(&graph, "route", 11);
    assert_eq!(route.category, "route_helper");
    assert_eq!(route.resolved.as_deref(), Some("route::users.store"));
    let title = find(&graph, "__", 6);
    assert_eq!(title.category, "i18n_helper");
    assert_eq!(title.resolved.as_deref(), Some("i18n::auth.title"));
    let lang = find(&graph, "lang", 13);
    assert_eq!(lang.category, "i18n_helper");
    assert_eq!(lang.resolved.as_deref(), Some("i18n::auth.failed"));
    assert_eq!(find(&graph, "@csrf", 12).category, "framework_use");
}

#[test]
fn directives_become_control_flow_with_branches() {
    let graph = page();
    let flows: Vec<String> = graph
        .control_flow
        .iter()
        .map(|f| {
            format!(
                "{} L{} {} {:?}",
                f.kind,
                f.line,
                f.cond.as_deref().unwrap(),
                f.branches
                    .iter()
                    .map(|b| (b.start_line, b.end_line))
                    .collect::<Vec<_>>()
            )
        })
        .collect();
    assert_eq!(
        flows,
        [
            "if L8 auth [(8, 9)]",
            "foreach L15 $users as $user [(15, 16)]"
        ]
    );
    assert_eq!(
        graph.control_flow[0].caller.as_deref(),
        Some("section:content")
    );
}

#[test]
fn if_elseif_else_has_one_branch_per_clause() {
    let graph = analyze(
        "resources/views/x.blade.php",
        "@if ($user->isAdmin())\n a\n@elseif ($user->isEditor())\n b\n@else\n c\n@endif\n",
    );
    let flow = &graph.control_flow[0];
    let labels: Vec<(&str, u32, u32)> = flow
        .branches
        .iter()
        .map(|b| (b.label.as_str(), b.start_line, b.end_line))
        .collect();
    assert_eq!(
        labels,
        [
            ("if ($user->isAdmin())", 1, 2),
            ("elseif ($user->isEditor())", 3, 4),
            ("else", 5, 6)
        ]
    );
    assert_eq!(find(&graph, "isAdmin", 1).line, 1);
    assert_eq!(find(&graph, "isEditor", 3).line, 3);
}

#[test]
fn layout_yields_stacks_and_includes() {
    let graph = analyze("resources/views/layouts/app.blade.php", LAYOUT);
    assert_eq!(graph.containers[0].name, "layouts.app");
    assert_eq!(find(&graph, "section:title", 3).category, "view_use");
    assert_eq!(find(&graph, "stack:scripts", 4).category, "view_use");
    assert_eq!(find(&graph, "section:content", 8).kind, "yield");
    let include = find(&graph, "partials.nav", 7);
    assert_eq!(
        include.resolved.as_deref(),
        Some("resources/views/partials/nav.blade.php")
    );
}

#[test]
fn anonymous_component_props_are_properties() {
    let graph = analyze("resources/views/components/alert.blade.php", ALERT);
    assert_eq!(graph.containers[0].name, "components.alert");
    let props: Vec<(String, Option<String>, u32)> = graph
        .properties
        .iter()
        .map(|p| (p.name.clone(), p.default.clone(), p.line))
        .collect();
    assert_eq!(
        props,
        [
            ("type".to_owned(), Some("'info'".to_owned()), 1),
            ("message".to_owned(), None, 1)
        ]
    );
    assert!(graph.methods.is_empty(), "{:?}", graph.methods);
}

#[test]
fn slots_are_blocks_and_php_blocks_are_analyzed() {
    let graph = analyze(
        "resources/views/card.blade.php",
        "<x-card>\n    <x-slot:header>\n        Title\n    </x-slot>\n    @include('partials.nav')\n    @php\n        $n = count($items);\n    @endphp\n</x-card>\n",
    );
    let methods: Vec<String> = graph
        .methods
        .iter()
        .map(|m| format!("{} L{}-{}", m.name, m.line, m.end_line.unwrap()))
        .collect();
    assert_eq!(methods, ["template L1-9", "slot:header L2-4"]);
    assert_eq!(
        find(&graph, "x-card", 1).caller.as_deref(),
        Some("template")
    );
    assert_eq!(find(&graph, "count", 7).line, 7);
}

#[test]
fn broken_and_empty_templates_do_not_panic() {
    for text in [
        "",
        "@section('a')\n{{ $x->(\n",
        "@if(\n<x-alert :a=\"$b->\n",
        "{{ }} @php\n$x = ;\n",
        "@foreach($a as\n@endforeach @endsection @else\n",
    ] {
        let graph = analyze("resources/views/b.blade.php", text);
        assert_eq!(graph.containers[0].kind, "view");
    }
}

#[test]
fn view_refs_list_the_used_views() {
    let refs = view_refs("@extends('layouts.app')\n@include('partials.nav')\n<x-forms.input />\n");
    let list: Vec<(String, u32, &str)> = refs
        .iter()
        .map(|r| (r.view.clone(), r.line, r.how))
        .collect();
    assert_eq!(
        list,
        [
            ("layouts.app".to_owned(), 1, "@extends"),
            ("partials.nav".to_owned(), 2, "@include"),
            ("components.forms.input".to_owned(), 3, "<x-…>")
        ]
    );
}

#[test]
fn registry_picks_blade_before_php() {
    let registry = crate::lang::LanguageRegistry::with_defaults();
    let blade = registry
        .for_path(std::path::Path::new("resources/views/a.blade.php"))
        .unwrap();
    assert_eq!(blade.id(), "blade");
    let php = registry
        .for_path(std::path::Path::new("app/Models/A.php"))
        .unwrap();
    assert_eq!(php.id(), "php");
}

#[test]
fn a_dynamic_component_has_no_view_and_directive_uses_are_listed() {
    let refs = view_refs("<x-dynamic-component :component=\"$w\" />\n<x-card />\n");
    let list: Vec<(String, u32)> = refs.iter().map(|r| (r.view.clone(), r.line)).collect();
    assert_eq!(
        list,
        [(String::new(), 1), ("components.card".to_owned(), 2)]
    );
    let uses = directive_uses("<p>@datetime($at)</p>\n<b @click=\"x\">@lang('a.b')</b>\n");
    let names: Vec<(&str, u32)> = uses.iter().map(|(n, l)| (n.as_str(), *l)).collect();
    assert_eq!(names, [("datetime", 1), ("click", 2), ("lang", 2)]);
}

#[test]
fn vite_entries_are_import_edges() {
    let graph = analyze(
        "resources/views/layouts/app.blade.php",
        "<head>@vite(['resources/css/app.css', 'resources/js/app.ts'])</head>\n@vite('resources/js/admin.ts')\n@vite($dynamic)\n",
    );
    let edges: Vec<(&str, &str, &str)> = graph
        .calls
        .iter()
        .filter(|c| c.kind == "import")
        .map(|c| (c.target.as_str(), c.category.as_str(), c.kind.as_str()))
        .collect();
    assert_eq!(
        edges,
        [
            ("resources/css/app.css", "module_use", "import"),
            ("resources/js/app.ts", "module_use", "import"),
            ("resources/js/admin.ts", "module_use", "import")
        ]
    );
    // the plain `@vite` marker call stays
    assert!(graph.calls.iter().any(|c| c.target == "@vite"));
}

#[test]
fn livewire_directive_is_a_component_use_and_events_get_the_key() {
    let graph = analyze(
        "resources/views/page.blade.php",
        "<div>\n@livewire('counter', ['start' => 1])\n@livewire($dynamic)\n<button wire:click=\"$dispatch('saved')\">x</button>\n<script>Livewire.on('refresh-list', () => {})</script>\n</div>\n",
    );
    let uses: Vec<(&str, &str)> = graph
        .calls
        .iter()
        .filter(|c| c.category == "component_use")
        .map(|c| (c.target.as_str(), c.kind.as_str()))
        .collect();
    assert_eq!(uses, [("livewire:counter", "component")]);
    let keys: Vec<(&str, u32)> = graph
        .calls
        .iter()
        .filter(|c| c.kind == "event")
        .map(|c| (c.target.as_str(), c.line))
        .collect();
    assert_eq!(
        keys,
        [
            ("livewire::event:saved", 4),
            ("livewire::event:refresh-list", 5)
        ]
    );
    // plain Alpine `$dispatch` in a template that never mentions Livewire is no Livewire event
    let alpine = analyze(
        "resources/views/plain.blade.php",
        "<button x-on:click=\"$dispatch('saved')\">x</button>\n",
    );
    assert!(alpine.calls.iter().all(|c| c.kind != "event"));
}
