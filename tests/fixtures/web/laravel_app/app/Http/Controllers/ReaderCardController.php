<?php

namespace App\Http\Controllers;

class ReaderCardController extends Controller
{
    public function index()
    {
        return inertia('Catalogue');
    }

    public function store()
    {
        return redirect()->route('cards.index');
    }

    public function edit(string $card)
    {
        return inertia('Catalogue', ['card' => $card]);
    }
}
