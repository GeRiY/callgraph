<div>
    <h1>{{ $count }}</h1>
    <input type="text" wire:model.live.debounce.300ms="label">
    <input type="number" wire:model="cnt">
    <button wire:click="increment">+</button>
    <button wire:click="dec">-</button>
    <button wire:click="$refresh">Reload</button>
    <button wire:click="$set('count', 0)">Reset</button>
    <button wire:click="$toggle('open')">Toggle</button>
    <button wire:click="$dispatch('counter-reset')">Reset event</button>
    <button wire:click="$parent.refresh()">Parent</button>
    <form wire:submit="save"></form>
</div>
