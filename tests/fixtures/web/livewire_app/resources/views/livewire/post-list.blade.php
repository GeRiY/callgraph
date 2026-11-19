<div>
    <input wire:model.live="search" placeholder="Search">
    @foreach ($this->posts as $post)
        <div wire:key="{{ $post->id }}">
            {{ $post->title }}
            <button wire:click="delete({{ $post->id }})">x</button>
        </div>
    @endforeach
    <livewire:create-post />
</div>
