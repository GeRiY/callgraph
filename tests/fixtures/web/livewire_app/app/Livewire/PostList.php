<?php

namespace App\Livewire;

use App\Models\Post;
use Livewire\Attributes\Computed;
use Livewire\Attributes\On;
use Livewire\Attributes\Url;
use Livewire\Component;

class PostList extends Component
{
    #[Url]
    public string $search = '';

    #[Computed]
    public function posts()
    {
        return Post::where('title', 'like', "%{$this->search}%")->get();
    }

    #[On('post-saved')]
    #[On('refresh-list')]
    public function refreshList(): void
    {
    }

    public function delete(int $id): void
    {
        Post::destroy($id);
        $this->dispatch('post-deleted');
    }

    public function render()
    {
        return view('livewire.post-list');
    }
}
