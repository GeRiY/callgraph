<?php

use App\Livewire\Dashboard;
use Illuminate\Support\Facades\Route;

Route::view('/', 'welcome');

// full-page component
Route::get('/dashboard', Dashboard::class)->name('dashboard');
