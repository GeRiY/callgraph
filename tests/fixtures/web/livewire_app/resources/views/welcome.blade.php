<html>
<body>
    <livewire:counter />
    <livewire:admin.user-table />
    @livewire('post-list')
    <livewire:is :component="$name" />
    {{-- <livewire:ghost /> --}}
</body>
</html>
