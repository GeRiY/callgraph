<?php

namespace App\Http\Controllers;

use Inertia\Inertia;

class EventController extends Controller
{
    public function show(int $event)
    {
        return Inertia::render('Events/Show', [
            'event' => ['id' => $event, 'title' => 'Opening night'],
            'can' => true,
        ]);
    }
}
