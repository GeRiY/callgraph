<?php

use App\Http\Controllers\CatalogueController;
use App\Http\Controllers\ReaderCardController;
use App\Http\Controllers\ShelfController;
use App\Http\Middleware\EnsureReaderCard;
use Illuminate\Support\Facades\Route;

Route::get('/shelves', [ShelfController::class, 'index'])->name('shelves.index');
Route::post('/shelves', [ShelfController::class, 'store'])->name('shelves.store');

Route::middleware(EnsureReaderCard::class)->group(function () {
    Route::get('/catalogue', CatalogueController::class)->name('catalogue.show');
});

foreach (config('library.pages') as $name => $page) {
    Route::get($page['uri'], $page['action'])->name($name);
}

Route::middleware(['auth', 'verified'])->prefix('cards')->name('cards.')->controller(ReaderCardController::class)->group(function () {
    Route::get('/', 'index')->name('index');
    Route::post('/', 'store');

    Route::prefix('{card}')->name('item.')->middleware('can:renew')->group(fn () => [
        Route::get('/edit', 'edit')->name('edit'),
    ]);
});
