<?php

namespace App\Livewire;

use Livewire\Attributes\Validate;
use Livewire\Component;

class Counter extends Component
{
    #[Validate('required|min:1')]
    public int $count = 0;

    public string $label = '';

    protected string $secret = 'x';

    public function increment(): void
    {
        $this->count++;
        $this->dispatch('counter-changed', count: $this->count);
    }

    public function save(): void
    {
        $this->validate();
        $this->dispatch('post-saved');
    }

    public function render()
    {
        return view('livewire.counter');
    }
}
