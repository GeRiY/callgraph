<?php

namespace App\Livewire\Admin;

use Livewire\Component;

class UserTable extends Component
{
    public string $role = 'all';

    public function setRole(string $role): void
    {
        $this->role = $role;
    }

    public function rules(): array
    {
        return [
            'role' => ['required', 'in:all,admin'],
        ];
    }
}
