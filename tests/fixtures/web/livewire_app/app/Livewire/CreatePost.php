<?php

namespace App\Livewire;

use App\Livewire\Forms\PostForm;
use App\Models\Post;
use Livewire\Component;

// no render(): the template is resources/views/livewire/create-post.blade.php
class CreatePost extends Component
{
    public PostForm $form;

    public function save(): void
    {
        $this->form->validate();
        Post::create($this->form->all());
        $this->dispatch('post-saved');
        $this->dispatch('refresh-list');
    }

    public function notify(string $kind): void
    {
        $this->dispatch('flash-' . $kind);
    }
}
