<?php

namespace App\Livewire;

use App\Models\Post;
use Livewire\Component;

class Dashboard extends Component
{
    public function render()
    {
        return view('livewire.dashboard', ['total' => Post::count()])->layout('layouts.app');
    }
}
