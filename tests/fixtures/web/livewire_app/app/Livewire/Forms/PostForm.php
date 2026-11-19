<?php

namespace App\Livewire\Forms;

use Livewire\Attributes\Validate;
use Livewire\Form;

class PostForm extends Form
{
    #[Validate('required|min:3')]
    public string $title = '';

    #[Validate('max:500')]
    public string $body = '';
}
