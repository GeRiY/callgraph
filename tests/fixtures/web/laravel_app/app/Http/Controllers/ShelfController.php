<?php

namespace App\Http\Controllers;

use App\Enums\LoanState;
use App\Http\Requests\StoreShelfRequest;
use App\Jobs\BuildOverdueDigest;
use App\Models\Shelf;
use Inertia\Inertia;

class ShelfController extends Controller
{
    public function index()
    {
        $upcoming = Shelf::query()
            ->stocked()
            ->orderBy('title')
            ->get();

        $closed = Shelf::where('state', LoanState::Returned->value)->first();

        return Inertia::render('Shelves/Browse', [
            'shelves' => $upcoming,
            'closed' => $closed,
        ]);
    }

    public function store(StoreShelfRequest $request)
    {
        $shelf = Shelf::create($request->validated());
        BuildOverdueDigest::dispatch($shelf->id);
        $request->invalidate();

        return redirect()->route('shelves.index');
    }
}
