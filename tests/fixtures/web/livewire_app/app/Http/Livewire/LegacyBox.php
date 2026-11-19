<?php

namespace App\Http\Livewire;

use Livewire\Component;

// Livewire 2 style
class LegacyBox extends Component
{
    protected $listeners = ['post-saved' => 'refreshBox', 'refresh-list' => 'refreshBox'];

    public $items = [];

    public function refreshBox()
    {
        $this->items = [];
        $this->emit('legacy-ping');
    }

    public function render()
    {
        return view('livewire.legacy-box');
    }
}
