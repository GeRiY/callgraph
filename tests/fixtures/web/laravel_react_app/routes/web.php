<?php

use App\Http\Controllers\EventController;
use Illuminate\Support\Facades\Route;

Route::get('/events/{event}', [EventController::class, 'show'])->name('events.show');
