@extends('layouts.app')

@section('content')
    @include('partials.gone')
    @include('mail::message')
    @include('partials.row')
    <x-card />
    <x-ghost />
    <x-dynamic-component :component="$widget" />
    <p>@datetime($at)</p>
    <button @click="open = true">Open</button>
@endsection
