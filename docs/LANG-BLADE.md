# LANG-BLADE — a Blade (Laravel sablon) frontend mezőtérképe

Modul: `src/lang/blade/` (`BladeFrontend`, `LanguageRole::Frontend`, azonosító `blade`, fájlok: `*.blade.php`). A
`.blade.php` kiterjesztése `php`, ezért a registry a fájlnév végére illeszt, és a PHP elé teszi: a Blade külön nyelv,
nem PHP-fájl. A sablon `{{ }}`, `@php` és direktíva-argumentum részei a meglévő PHP-elemzőn mennek át, sorhelyesen
(a hívás sora a sablon sora). Hibatűrő: a hibás sablon is gráfot ad, ami nem ismerhető fel, kimarad.

## Fájl-szintű keret
- **Container**: fájlonként egy `view`, neve a `resources/views`-hoz relatív pontozott nézet-név
  (`admin/users/index.blade.php` → `admin.users.index`; az anonim komponens `components.alert`). Az `@extends('x')` a
  container `extends`-je (`view admin.users.index extends layouts.app`).
- **Metódusok** (blokkok, `()` nélkül, sortartománnyal): `section:content` (`@section('content')` … `@endsection` /
  `@show` / `@stop` / `@append`), `push:scripts` (`@push`, `@pushOnce`, `@pushIf`), `prepend:x`, `slot:header`
  (`@slot('header')` és `<x-slot:header>` / `<x-slot name="header">`), és a szakaszokon kívüli rész `template`
  néven (csak ha van benne hívás vagy elágazás). Az egysoros `@section('title', 'Users')` nem blokk.
- **Property-k**: a `@props(['type' => 'info', 'message'])` kulcsai alapértékkel (anonim komponensekhez).

## Hívások
| Kategória | Mikor |
|---|---|
| `view_use` | `@extends`, `@include` / `@includeIf` / `@includeWhen` / `@includeFirst`, `@each` — a cél fájlra feloldva (`resources/views/layouts/app.blade.php`); `@yield('x')` (`section:x`) és `@stack('x')` (`stack:x`) |
| `component_use` | `<x-alert>`, `<x-forms.input>` (anonim sablonra: `resources/views/components/forms/input.blade.php`, ha a lemezen az osztályos `app/View/Components/…` létezik, arra), `<livewire:counter>` (`app/Livewire/Counter.php`, ha megvan), `@component` |
| `route_helper` | `route('users.store')` — `resolved`: `route::users.store`; az outline a route-táblából írja a linket (`→ POST /users · UserController@store`, hiányzó névnél `⚠ no route`), és a `--format ziggy` ismeretlen nevet jelez |
| `i18n_helper` | `__('k')`, `trans('k')`, `trans_choice`, `Lang::get`, `@lang('k')`, `@choice('k')` — `resolved`: `i18n::k`; hiányzó kulcsra `⚠ missing in hu.json`; a használat a nyelvfájl „nem használt” számlálójába is beleszámít |
| `framework_use` | `@csrf`, `@method('PUT')`, `@vite(...)` |
| (PHP-kategóriák) | a `{{ $order->total() }}`, `{!! Str::limit(...) !!}`, `auth()->user()`, `config('x')`, `@php … @endphp` és a `:message="$e->first()"` kötött komponens-attribútum hívásai a PHP szabályai szerint |

## Vezérlési ágak
`@if` / `@elseif` / `@else`, `@unless`, `@isset`, `@empty`, `@auth`, `@guest`, `@can` / `@cannot` / `@canany`,
`@env`, `@hasSection`, `@switch` / `@case` → `if` / `switch` ágak sortartománnyal; `@foreach` / `@forelse` (az
`@empty` ágával) / `@for` / `@while` → egy-ágú ciklus-szerkezet. A feltétel hívásai (`@if ($user->isAdmin())`) hívásként is
megjelennek.

## Laravel-kapcsolatok
- **`rendered by:`** a Blade-fájl outline-jában: a `view('admin.users.index')`, `View::make`, `Route::view`, Mailable
  `->view()` / `->markdown()` hívó osztálya és metódusa (`UserController::index L8 (+N)`); **`used by:`** a többi
  sablon `@extends` / `@include` / `@each` / `@component` / `<x-…>` hivatkozása (`admin.users.index L1 @extends`).
  Dinamikus nevű render kimarad.
- **Nézet-hivatkozás fájlra**: a `view_use` hívás `resolved`-je a fájl útja, így `--grep layouts/app.blade.php` (vagy
  `layouts.app`) a layout-ot használó nézeteket adja, és a `--depth` követi őket.
- **`--format views`** a Blade-fájlnál a nézet-használatot (`@extends`, `@include`, `<x-…>`) listázza.

## Korlátok
- A direktívák ismert körét ismeri; egyedi (`Blade::directive`) direktíva törzse nem elemződik.
- A `@verbatim` blokk, a `{{-- --}}` komment és az `@{{ }}` escape kimarad.
- A `{{ $x or 'y' }}` típusú, nem érvényes PHP-kifejezés kimarad (a többi kifejezés érintetlen).
- Egy `@include` / `<x-…>` célja nem létező fájl is feloldódik útra (a hiányát az outline nem jelzi).
- A dinamikus nézet-név (`@include($view)`), a `<x-dynamic-component>` és a csomag-névterű (`mail::x`) nézet nem
  oldódik fel fájlra.
