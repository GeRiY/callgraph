# callgraph — generate, rename, undo

`CG=<skill>/callgraph.sh`, as in [SKILL.md](../SKILL.md) (the core: start there).

## Generate and undo (writes files; opt-in over MCP)

`gen '<expr>'` writes code from a short expression and prints one line; it never overwrites a file, backs up every change
first and parses the whole new text before writing (on a syntax error nothing is written). `undo` is a stack: each call steps one write further back (last 20 per file; nothing left → a clear message).

```bash
bash $CG gen 'c!:Pay<Base> > ctor(-Client $http) + +charge(Order $o):Receipt' --write app/Pay.php --project myapp   # new file
bash $CG gen '+refund(Order $o):void' --into app/Pay.php::Pay --after charge --project myapp                    # add a method
bash $CG gen '+charge(Order $o):Receipt{ ... }' --replace app/Pay.php::charge --project myapp                   # replace a method
bash $CG gen 'c:Pay' --dry --project myapp                                                                      # preview: the code only
bash $CG undo --project myapp                                                      # restore the last change (a preset / rename: all its files)
```

Rename (one call, one `undo`; `--dry` shows files moved, sites per file, flagged strings and the migration to be created):
`rename route 'a.*' 'b.*'` (route names), `rename class 'App\Models\Fee' 'App\Models\Charge'` (one class), `rename method 'App\Pay::charge' bill`,
`rename term post article` (a whole concept in every spelling: `Post`/`posts`/`post_id`/`PostController`/`/posts/{post}`/`admin.posts.index`/`Posts/Index.vue`;
files whose path has the word are moved; tables and `*_id` columns get a new `rename_post_to_article` migration, old migrations stay; skips `POST`, `->post(`/`.post(`/`postJson(` calls; a bare `'post'` string is only flagged; irregular plural: `person/people`).

Expression grammar: `c:` class `i:` interface `t:` trait `e:` enum; `>` child, `+` sibling, `^` up, `*n` repeat;
members `+`/`-`/`#` visibility, `::` static, `name(Type $p):Type{body}`, `$prop:Type=1`, `K=1`, `ctor(-Type $x)` promoted.
Unknown class names are imported from the project (`unresolved:` lists the ones it could not place). `--dry` prints the
code; with `--into` / `--replace` a first `→ file::Class after method (L41)` line says where it would go.
`--replace ...:L42-L87 --expect <sha256-prefix>` refuses if those lines changed since you read them.
Placement: `--into <file>::Class` takes `--after <method>`, `--before <method>` or `--at L<n>`; `--into x.vue::template#id` appends to that element; `--upsert` replaces the member if it already exists. `--replace x.vue::template` / `::script` / `::style` swaps a whole section (tags stay); `--diff` (with `--replace`) prints only the old / new lines of the change instead of the new code (nothing is written).
MCP `generate` parameters: `expr`, `dry`, `write`, `into`, `after`, `before`, `at`, `upsert`, `diff`, `replace`, `expect`, `project`.

Also Vue / JS / TS / HTML (by the file extension) and `laravel:Fee>create(name:str)+index` (a whole feature, one undo).
PHP: `use:HasFactory,Notifiable` under a type = trait use inside the class (`c:Post<Model> > use:HasFactory + +comments():HasMany{…}`);
`a:<Migration> > +up():void{…}` = anonymous class (`return new class extends Migration {…};`, Laravel migrations); in a Laravel project
(`laravel/framework` in composer.json) unknown short names (`Model`, `Schema`, `Blueprint`, `Route`, `HasMany`, `Request`, `Rule`, `RefreshDatabase`, …) are imported without `use:`.
Statements without a class: `php{ Route::get('/posts', [PostController::class, 'index']); }` with `--write routes/x.php` (adds `<?php` + imports),
`--into routes/web.php` (end of file, or `--at L<n>` / `--after L<n>`), or `--replace bootstrap/app.php:L14-L16` (the block takes the indentation of the line it goes at / replaces; an empty `php{}` with `--replace` deletes the range); `raw{…}` writes the text verbatim (e.g. `--write resources/views/app.blade.php`, not syntax-checked).
MCP: the `generate`, `rename` and `undo` tools are listed only when the server runs with `--generate` (`CALLGRAPH_GENERATE=1`).
