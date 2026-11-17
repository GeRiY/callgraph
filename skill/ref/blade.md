# callgraph — Blade

`CG=<skill>/callgraph.sh`, as in [SKILL.md](../SKILL.md) (the core: start there).

## Blade (`*.blade.php`)

Own language, not a PHP file. Container: one `view` per file, named by the dotted path under `resources/views`
(`admin/accounts/index.blade.php` → `admin.accounts.index`; an anonymous component `components.alert`); `@extends('x')` is its
`extends`. "Methods" are the blocks, with line ranges: `section:content`, `push:scripts` (`@push`, `@pushOnce`, `@pushIf`),
`prepend:x`, `slot:header` (`@slot` and `<x-slot:header>`), and the part outside sections as `template` (only if it has a
call or a branch). A one-line `@section('title', 'x')` is not a block. `@props([...])` keys are properties.
Call categories: `view_use` (`@extends`, `@include*`, `@each`: resolved to the target file; `@yield` → `section:x`, `@stack` →
`stack:x`), `component_use` (`<x-alert>`, `<x-forms.input>` → the anonymous template or `app/View/Components/…`, `<livewire:counter>`,
`@component`), `route_helper` (`route('users.store')` → `route::users.store`; the outline shows `→ POST /users · UserController@store`
or `⚠ no route`), `i18n_helper` (`__()`, `trans()`, `@lang`, `@choice` → `i18n::k`; `⚠ missing in hu.json`), `framework_use`
(`@csrf`, `@method`, `@vite`). `{{ }}`, `@php` and bound attributes (`:message="$e->first()"`) follow the PHP rules, with the template's line numbers.
Branches: `@if` / `@unless` / `@isset` / `@auth` / `@can` / `@env` / `@switch` → `if` / `switch` with line ranges; `@foreach` /
`@forelse` / `@for` / `@while` → a one-branch loop.
Laravel links: the outline of a Blade file shows `rendered by:` (the class and method calling `view('…')`, `View::make`,
`Route::view`, `->view()` / `->markdown()`) and `used by:` (other templates via `@extends` / `@include` / `<x-…>`);
`--grep layouts/app.blade.php` (or `layouts.app`) lists the views using a layout, `--depth` follows them;
`--format views` on a Blade file lists its view usage. Not covered: custom `Blade::directive` bodies, `@verbatim`, `{{-- --}}`,
dynamic view names (`@include($view)`), `<x-dynamic-component>`, package-namespaced views (`mail::x`); a missing target file still resolves to a path.
