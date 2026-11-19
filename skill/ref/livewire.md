# callgraph — Livewire

`CG=<skill>/callgraph.sh`, as in [SKILL.md](../SKILL.md) (the core: start there). Livewire 2 and 3 components (`class X extends Component`,
`app/Livewire` or `app/Http/Livewire`) and the Blade templates they render. Volt (anonymous classes in a Blade file) is not read.

- **Overview**: `$CG app --format livewire` (MCP `framework_view`, `view: "livewire"`): a block per component, at most four lines:
  `Counter  app/Livewire/Counter.php → livewire/counter.blade.php · used: 2 · route GET /dashboard`, then `props: count, search (url) · actions: increment, save`,
  `events: dispatches saved · listens refresh-list`, `rules: name*, email` (`*` = required; from `#[Validate('required|min:3')]`, `rules()`, `$rules`,
  `$this->validate([...])`, and a Form object as `form.title*`), and `⚠` lines. A header counts components, uses and warnings; a `? N skipped: …` footer
  lists names built at run time (`<livewire:is>`, `@livewire($name)`, `dispatch('x' . $y)`, `getListeners()`).
- **Class ↔ template ↔ uses**: the template is the first `view('livewire.x')` literal in `render()`, or `resources/views/livewire/<kebab>.blade.php`
  when there is no `render()`. Uses: `<livewire:x>`, `<livewire:dir.x>`, `@livewire('x')`, `@livewire(X::class)`, `Route::get('/x', X::class)`.
  The outline of the class shows `view: …` and `used in: file Lnn, …`; the outline of the template shows `livewire: Counter (app/Livewire/Counter.php)`,
  `calls: increment L5, save L12`, `binds: label, cnt` and the findings.
- **Contract**: `wire:model[.live…]="prop"` must be a public property (`form.title`: a property of the Form object), `wire:click|submit|keydown|…="method(args)"` a public method:
  `⚠ wire:click: no public method 'dec' in Counter  <template> L6`. Never reported: `$refresh`, `$set`, `$toggle`, `$dispatch`, `$parent.x()`, JS expressions,
  Blade-built names, and every class that uses a trait or `__call` (members not fully visible). `{{ $prop }}` is not checked.
- **Events**: `$this->dispatch('x')`, `$dispatch('x')`, `wire:click="$dispatch('x')"`, v2 `emit('x')` ↔ `#[On('x')]`, v2 `$listeners`: one key `livewire::event:x` as a
  `framework_use` call, so `$CG app resources --grep livewire::event:x` finds the dispatchers and the listeners.
- **Trace**: `$CG trace livewire:counter` (also `Counter`, `livewire:admin.user-table`): the class, the route of a full-page component, the template, where it is used,
  props, actions, events, rules, the models the class names, findings.
- Not covered: Volt, `Livewire::component()` aliases, custom view paths, `{{ $prop }}` checks, parameters passed to a child, event-without-listener warnings.
