<?php

use App\Http\Controllers\PageController;
use Illuminate\Support\Facades\Route;

Route::get('/home', [PageController::class, 'home'])->name('home');

foreach (Menu::all() as $entry) {
    Route::get('/menu/' . $entry->slug, [PageController::class, 'show'])->name($entry->slug);
}
