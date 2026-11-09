# Changelog

All notable changes to callgraph. Work is collected in packages on the `develop` branch and released together.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versions: [SemVer](https://semver.org/).

## [Unreleased]

## [0.5.3] — 2026-11-09

### Added
- **`--strict` for CI:** a file with a syntax error still gives exit code 0 by default (the partial map is useful).
  With `--strict` (or `CALLGRAPH_STRICT=1`), `show` (file, folder, `--grep`), `locate` and `profile` exit with code 3
  when a file they read has a syntax error; the output is the same.
- **`mail` and `notification` layers:** Laravel mail classes (`extends Mailable`, usually in `app/Mail`) and
  notifications (`extends Notification`) get their own layer instead of "unknown". Like `event`, they need this proof
  in the code: a folder that is just named `Mail` is not enough.

### Changed
- **The `flow:` line only shows real call pairs:** every `A → B` in the chain now means A really calls B. A layer
  that only calls into the middle of the chain is shown as a side branch, e.g.
  `flow: controller → service → model; command → service` (before: `controller → command → service → model`, although
  controllers never call commands).
- **Enums get no role from their name:** `app/Enums/PaymentProvider.php` was a "provider" just because of its name.
  An enum (PHP, TS, Rust, Swift) never gets a role from its name now; an `Enums` folder is shown as `enum`.
- **Copied framework cores are skipped like vendor code:** a framework copied into a legacy project (CodeIgniter
  `system/`, Yii `framework/`, Kohana `system/`) no longer adds `command` / `handler` / `request` layers to the
  profile. Only the framework core is skipped, the application code next to it still counts. A big subtree you want
  left out completely can go into `.callgraphignore` as a path (`sn_monitoring/`).

### Fixed
- **Syntax errors no longer hide the code after them (JS / TS / Vue, Rust):** before, only the functions before the
  error were mapped; in a Vue `<script setup>` an error in the first function could leave nothing. Now mapping goes
  on from the next top-level function, class or item, with the right line numbers; only the broken part is missing.
- **`--layer` / `--layers` / `--by-layer` now agree with the profile:** they classified files from folder names only,
  so `app/Actions/Event/*Action.php` showed up as `[event]` although the profile did not list an event layer. They now
  use the same rules and code proof as `profile` (from the project map; without it, a folder like `Event` gives the
  next role, e.g. `service`). Files skipped as vendor or build output get no layer here either.
- **`callgraph.sh --project <dir>`:** a folder path (absolute or relative) now works with every command, without
  registering it; before, file queries said "No projects.json — register a project first". An unknown alias still
  gives an error.

## [0.5.2] — 2026-11-09

### Added
- **`update --dev`:** installs the newest build of the `develop` branch (version `X.Y.Z-dev.<commit>`, checked
  against its `SHA256SUMS` like a release); `update --check --dev` only tells you whether there is a newer one. A plain
  `update` takes you back to the latest stable release. The daily background check and the MCP `--auto-update` stay
  on stable releases. On a dev build `callgraph --version` shows the dev version.

### Changed
- **The changelog in Hungarian too:** `CHANGELOG.hu.md` has every release in Hungarian, and the Hungarian website
  shows it.

### Fixed
- **`update --check` right after a release:** for a few minutes after a new release it could still say "up to date",
  because GitHub briefly served the old answer. It now asks without the cache and also looks at the list of recent
  releases, so a new version is found immediately.

## [0.5.1] — 2026-11-09

### Added
- **Local usage stats:** `callgraph.sh stats` shows how you (and your agents) used callgraph in the last 7 days
  (`--days N` for up to 90): how many calls per command and MCP tool, how many failed, how long they took and how big
  the output was, and for `locate` how often the first result was the file the agent opened next. Everything stays on
  your computer: nothing is sent anywhere, and no paths, names or search terms are recorded — only counts in ranges.
  The data is in the skill folder (`stats/`, about 90 days); turn it off with `CALLGRAPH_STATS=0`.
- **`--no-gitignore`:** add it to any command (show, folder views, `locate`, `profile`, `refresh`, `callers`) to also
  map the files your `.gitignore` excludes; for the MCP server and the hook set `CALLGRAPH_GITIGNORE=0`.
  `.callgraphignore` and the always-skipped folders (`vendor`, `node_modules`, …) still apply.
- **`locate route::events.show`:** finds a route by its name: the definition (`->name('events.show')` in
  `routes/*.php`) comes first, then every `route('events.show')`, `to_route(…)` and `redirect()->route(…)` in PHP and
  JavaScript / TypeScript / Vue. If nothing is found, it says what it looked for.
- **`callgraph.sh --version`** (also `-V` and `version`) prints the version, the platform and where the binary is.

### Changed
- **`.callgraphignore` accepts paths:** a line like `md`, `.md` or `*.md` still skips an extension, and every other
  line is now a path pattern written like in `.gitignore` (`public/build/`, `/storage/`, `!keep.js`), relative to the
  folder of the file. A bare word such as `build` is still an extension; write `build/` for a folder.
- **`locate` ranks exact matches first:** a file whose class, component or function name (or file name) is one of the
  terms now always comes before files that only contain the terms as text, such as translation files or build
  output.
- **A quoted phrase is one `locate` term:** `locate "Organizer's other events"` searches for the whole text in the
  strings of the code (labels, translations) instead of splitting it into words that matched unrelated names.
- **`locate` always shows a `root:` line** and paths relative to it, also when only one file matches.
- **The profile lists the main layers first:** the architecture layers (controller, service, model …) come first, by
  file count, and tests, migrations and configuration come last, so a main layer such as `model` no longer hides
  behind `… +6 more layers` while layers with one or two files are shown.
- **`--by-layer` shows a header for each layer:** every group of callers starts with a line such as
  `[service]  3 calls`, in the normal and in the short ("one line per call") `--grep` view, and in every level of
  `--depth`. The group closest to the called method comes first (model → repository → service → controller), as
  the help says; before, only a `[service]` tag at the end of some lines showed the layer.
- **User mistakes are no longer logged as tool errors:** a mistyped layer name, an unknown option or a file that does
  not exist now prints just the message — no `[error-id: …]` line, no bug-report link, and nothing is written to
  `errors.jsonl`. Real internal errors are still logged and get an error-id.
- **JSON outlines show the values:** `scripts.build = "vite build"` and `devDependencies.vite = "^6.0"` instead of
  just `string`; numbers, `true` / `false` and `null` as they are, strings over 60 characters cut with `…`, and
  objects and lists with their size (`object (3)`, `array (2)`).
- **Read one JSON key:** `callgraph.sh package.json::scripts --body` prints that key with its exact lines; use dots
  for deeper keys (`scripts.build`) and numbers for list items (`files.0`). Before, it said "No such method".
- **`locate` finds JSON keys:** `locate laravel-vite-plugin` now lists `package.json` with the line of the key. Only
  the exact key name (or the whole dotted path) counts, so searches for other words do not fill up with JSON files.
- **Markdown headings show their level:** the outline indents sub-headings and marks them with `#`, `##`, `###`, and
  no longer writes `()` after a heading (`## Docker / build  L51-61`).
- **CSS and SCSS rules are no longer written like functions:** `.p-toast  L29-31` instead of `.p-toast()`; mixins and
  functions keep their `()`.

### Fixed
- **CSS outlines list every rule:** some rules were missing (a rule with the selector `.p-toast` was always left out,
  and so was the first rule after `@charset`), and at-rules were not shown at all. Now every rule and at-rule is listed with its
  lines: `@import`, `@tailwind`, `@media`, `@supports`, `@layer`, `@keyframes`, `@font-face`, and the Tailwind v4
  `@theme`, `@utility`, `@custom-variant`, `@variant`, `@plugin`, `@source` and `@config`.
- **CSS rules start on the right line:** a rule after a comment or another rule showed the line where the previous
  rule ended (`.p-datepicker-panel  L31-41` instead of `L39-41`).
- **CSS paths with `/*` inside quotes stay whole:** `@source '../**/*.blade.php'` was cut, because the `/*` in the
  path was taken for the start of a comment.
- **PHP and JavaScript layers are no longer mixed up in the profile:** in a project with PHP controllers and
  AngularJS (or other JavaScript) controllers, the profile showed one `controller` layer with all of them. A role
  that exists in more than one language is now shown apart, e.g. `controller (php)` and `controller (js)`, and the
  call direction (`flow:`) no longer joins the backend and the frontend through a shared name such as `service`.
  Projects with one language look exactly as before. `--layer controller` still finds both; `--layer controller:js`
  (or `--layer "controller (js)"`) only the JavaScript ones.
- **Third-party code no longer makes up the project's architecture:** an SDK or library copied into the project (a
  subfolder with its own `composer.json`, `package.json` or `LICENSE`, or folders such as `vendors/`, `third_party/`,
  `extensions/`, `external/`, `libs/`) gave the profile layers and patterns (Builder, Factory, Handler…) that the
  project itself does not have. These files are now left out of the profile, and one line says so, e.g.
  `vendor: 312 files skipped (extensions/Facebook, vendors/phpoffice, …)`. Searches and folder views still see them.
  `profile --with-vendor` counts them again.
- **Copies of build output are left out of the profile:** folders named `dist`, `build` or `*.min` (such as
  `web/assets.min`) and minified files no longer count twice as layers; a `generated: N files skipped (…)` line shows
  them.
- **A folder named after a business topic is not an event layer:** `app/Services/Event` or a JavaScript
  `controllers/event` folder became an `event` layer. `event`, `listener` and `job` now need proof in the code — an
  Event / Listener / Subscriber base class or interface, Laravel's `Dispatchable` or `ShouldQueue`, or a `handle(…)`
  method in a `Listeners` namespace; otherwise the file keeps its other role (`service`, `controller`, `model` for
  `app/Models/Job.php`).
- **Files your `.gitignore` excludes no longer show up:** the build output of a Laravel + Vite project
  (`public/build/assets/app-CBW7lKWe.js`, listed in `.gitignore` as `/public/build`) appeared in `locate` and folder
  views. `.gitignore` now works the way git reads it: patterns with a leading `/`, nested paths, `**`, `!` exceptions
  and a `.gitignore` in any subfolder. The same rules apply everywhere callgraph looks at files, including the MCP
  server's file watcher and the editor hook.
- **Minified and bundled JavaScript / CSS is skipped even without a `.min.` name or a `.gitignore`:** a file whose
  first lines are extremely long is recognised as generated and left out.
- **`locate --orange-500`** (a CSS variable) was rejected as an unknown option; it is now searched for. The known
  options (`--all`, `--project`) still work as before.
- **`callgraph.sh --version`** printed the usage of `show` and logged an error.
- **Files with a syntax error are no longer shown as if they were complete:** a file you are in the middle of editing,
  or one with a merge conflict, could come back with no functions at all (JavaScript, TypeScript, Vue, Rust) or
  without any notice (PHP, Swift). A search then said "no such method" or found no callers, which could make an agent
  think the code was unused. Now the parts before the error are kept (functions `a` and `b` stay visible when `c` is
  broken), and every result says so: `⚠ syntax error at L3: … — this file's outline may be incomplete` under the file,
  a closing line such as `⚠ 2 files had syntax errors (results may be incomplete): src/tail.js L3, src/v.vue L4` when
  a folder, a search (`--grep`, `--depth`), `locate` or `profile` read a broken file, `parseErrors` in `--format
  json`, and a warning on the error output of `callers`. The exit code stays 0, because the rest of the result is
  still correct and useful. Covers JavaScript, TypeScript, Vue, PHP, Rust, Swift and JSON (CSS, SCSS and Markdown
  are read leniently and are not flagged).

## [0.5.0] — 2026-11-06

### Added
- **Where to start with a task — `locate`:** give it the names and terms from a task in one go (`callgraph.sh locate
  RefundController processRefund refund payment_status checkout.vue`, or the new `locate` tool over MCP) and it lists
  the few files worth opening first, best first, each with the reason: the class or method of that name, the file of
  that name, a name that contains the word (`refund` → `RefundJob`, `refunds_table`), a text such as a column name or
  route, or a call to it. It answers from the project map without reading the code again, so it is quick. A file that
  matches several of your terms comes before one that matches a single term many times. It shows 5 files (`--all`
  for every match) and says which terms found nothing and which are too general. The skill and MCP instructions now
  also tell agents when to use it, and when plain text search (grep) is the better tool.
- **Project profile:** `callgraph.sh profile` tells you in about 15 lines how an unfamiliar project is built: the
  architecture (Laravel / Yii layered MVC, DDD / hexagonal, Vue / React frontend, SwiftUI MVVM, Rust web), which folder
  holds which layer (controllers, services, repositories, models, components, stores…) with file counts, the design
  patterns it uses (Repository, Factory, Strategy, Observer…), the folders with no clear role, and whether the layers
  call each other in the expected direction. It reads folder and file names and checks them against the code graph
  (`extends Controller`, `implements *RepositoryInterface`, `defineStore`); what it cannot tell stays "unknown".
  `--full` shows every folder. For AI clients: the `project_profile` MCP tool, and a short profile is sent when a
  client connects.
- **Layers in queries (opt-in, nothing changes by default):** `--layer service` searches only the files of that layer
  (no need to know the folder names), `--layers` groups the summary by layer and marks each file with its role,
  `--by-layer` orders "who calls it?" results by layer, closest to the called code first (also for `callers`, and as
  `layer` / `by_layer` on the MCP tools). Folder names that say nothing, or are not English, can be mapped in a
  commit-able `.callgraph-layers` file (`service = app/Logic`, `ignore-role = app/Legacy`).
- **Exclude extensions per project:** a `.callgraphignore` file in the project root (or in any subfolder; the nearest
  one applies) lists extensions callgraph should skip there, one per line (`md`, `.json` or `*.vue`; `#` starts a
  comment). Commit it, and everyone on the team gets the same results. A file you name directly with an excluded
  extension is refused with a clear message.

### Changed
- **Only supported files are processed, more strictly:** callgraph already ignored files with other extensions; now
  it also skips generated files (`package-lock.json`, `*.min.js`, `*.min.css`) and files with binary content, even
  when their extension is supported. Naming one of them directly gives a clear reason instead of a result.
- **Upper-case extensions count:** `B.PHP`, `App.TS` or `Main.Vue` are now processed like their lower-case versions
  (before, they were silently skipped).

### Fixed
- **A file reached through a symlink is no longer listed twice:** if a symlink pointed to a file inside the project,
  that file appeared (and was analysed) twice in folder results.
- **Windows: the short summary shows file names again:** when a folder's output was too long and callgraph showed the
  one-line-per-file summary instead, Windows listed each file with its full path instead of just its name.

## [0.4.0] — 2026-11-06

### Added
- **Refresh the project map ahead of time:** `callgraph.sh refresh` brings the project map in `.callgraph/` up to
  date right away — every changed, new or deleted file, or only the files / folders you name — and prints a one-line
  summary. Handy after a `git pull` or after an agent wrote many files: the next query no longer has to re-analyse
  them. `--background` returns at once and does the work in a separate process. Two refreshes never work on the same
  project at the same time; if one is already running, the other simply skips.
- **Optional Claude Code hook:** after Claude edits or writes a file, callgraph refreshes that file in the background,
  so the next question about it is answered instantly. The installer asks whether to add it (`install.sh --hooks`
  adds it without asking); it only adds one entry to `~/.claude/settings.json`, keeps all other settings and hooks,
  and saves a `settings.json.bak` backup first. Check or remove it any time: `callgraph.sh hook status` /
  `callgraph.sh hook uninstall`. It does nothing outside registered projects (or ones that already have a
  `.callgraph/` folder).
- **The MCP server keeps itself up to date:** while `callgraph mcp` runs, it watches the projects it has opened and
  refreshes changed files a moment after they are saved, so queries stay fast. The answers are exactly the same as
  before. If watching is not possible on a system, it says so once and works as before. To switch it off, set
  `CALLGRAPH_WATCH=0`.
- **The MCP server explains itself:** AI tools that use callgraph through MCP (Claude Code, Claude Desktop, Cursor,
  VS Code) now get a short usage guide when they connect, in your language: which tool to use when, how to find every
  caller of a function, and how to read the answers. A new `guide` tool gives them the full manual on request. The
  tool descriptions now each carry an example and the most common mistake to avoid. The guide is made from the same
  text as the skill's `SKILL.md`, so the two always say the same thing.
- **The installer can set up the MCP server for Claude Code:** if the `claude` command is available, `install.sh`
  asks whether to register callgraph as an MCP server (`install.sh --mcp` does it without asking, `--no-mcp` skips
  it). Running it again does not create duplicates. It prints how to remove it (`claude mcp remove -s user callgraph`)
  and where to find the setup for Claude Desktop, Cursor and VS Code. If this step fails, the rest of the install
  still completes. The installer now ends with a reminder to restart Claude Code.
- **The MCP server can update itself:** add `--auto-update` to the server's arguments in your AI tool's MCP settings
  (`"args": ["mcp", "--auto-update"]`, recommended). When the tool starts the server — at most once a day — it checks
  for a newer callgraph release, downloads it, checks that the file is exactly the published one (it never installs
  a file whose checksum does not match), installs it just like `callgraph.sh update` (your projects and settings are
  kept) and restarts itself, so the tool works with the new version right away. If anything goes wrong (no
  internet, GitHub limit, a bad download, no write permission) it simply starts the version you have and writes one
  line to the tool's log. It only writes into callgraph's own folder, never into your projects.
- **The MCP server tells you about updates:** without `--auto-update`, the server checks for a newer release at most
  once a day in the background (it never makes a request wait) and adds one line to the first answer of the session:
  `ℹ callgraph X is available (installed: Y) — update: …/callgraph.sh update, or add --auto-update to the MCP
  config`. `CALLGRAPH_UPDATE_CHECK=0` turns off all update checks (no network at all); they are also off in CI.

### Changed
- **Comments inside a method stay in `--body`:** the short method view used to hide comment lines inside the method
  too, so an agent could think a line was missing and read the file again. Now only the docblock above the method
  and blank lines are left out; comments inside the method (they often explain the intent) are shown. When a
  docblock was left out, the header line says so, e.g. `(docblock L112-119 omitted: --doc)`. `--doc` still adds the
  docblock, `--raw` still prints the exact source. The MCP `method_body` tool works the same way.
- The project website has its own domain: https://callgraph.eu (the old `geriy.github.io/callgraph` address redirects).

### Fixed
- **"Who calls it" now finds calls from child classes:** searching for a method with its class name
  (`--grep FrontendController::redirectToNewDashboard`, or the MCP `find_callers` tool) missed the places where a
  child class calls the inherited method as `$this->redirectToNewDashboard()` — only the bare method name found them.
  Now these calls are found too, marked with the child class (`L139 via EventeditController │ …`). A child class that
  has its own version of the method is not counted, because its call runs its own version.

## [0.3.0] — 2026-11-06

### Changed
- **Faster queries on big projects:** callgraph now keeps a compact project map in `.callgraph/` in just three
  files, however big the project is: a short list of all files, one file with every file's full analysis, and one
  with the summary that searches need. It is updated file by file, only for files that changed since the last query
  (deleted or renamed files drop out). "Where is it called" (`--grep`, every `--depth` level), `callers`,
  `--strings` and `--const` are answered from the summary file in one read; a single-file view (outline, `--body` …)
  reads just that file's part of the full-analysis file. The output is exactly the same as before.
  An edited file shows up in the very next query; anything damaged or written by another callgraph version is
  rebuilt quietly. The map replaces the old analysis cache (`cache.jsonl`), which is removed automatically, so the
  folder takes less space than before. The MCP server (`callgraph mcp`) keeps the map in memory between requests.
  To switch it off (e.g. to compare), set `CALLGRAPH_NO_GRAPH=1`. Details: `docs/PROJECT-GRAPH.md`.
- **Shorter call lists in "where is it called" results:** when one caller method makes the same call on several lines
  with the very same source text, it is now shown once with all its line numbers, e.g.
  `L12, L40 │ $this->repo->find($id)`. Each caller shows at most 3 such rows; the rest are listed by line number only:
  `… +2 more: L55, L61`. This applies to `--grep`, the first `--depth` level and the one-line-per-call view used when
  the result would be too long. Calls through an interface stay on their own row (`via …`).
- **Shorter method bodies:** `--body` now leaves out what an agent rarely needs: the docblock / doc comments above the
  method, blank lines and lines that are only a comment. Code lines are printed exactly as written (a comment at the end
  of a code line stays), and nothing inside a string, heredoc or template literal is ever removed. The line numbers are
  still the real ones, so a gap shows where lines were skipped and you can still read an exact range. Attributes such as
  `#[test]` / `#[Route(...)]` are kept. Works for PHP, JS / TS / Vue, Rust and Swift, for several methods at once and
  for `--grep … --body`.
- New `--doc` adds the docblock back; new `--raw` prints the exact, unfiltered source (as before, plus Rust / Swift
  doc comments and attributes). The MCP `method_body` tool follows the same default and has `doc` / `raw` options.

### Added
- **Notes on files and methods (pilot):** `callgraph note add <file>[::<method>] "<text>"` (also
  `callgraph.sh note …`) attaches a short note, stored outside the source in `<project>/.callgraph/notes.jsonl`
  (git-ignored) with the date and time (UTC). The outline shows it under the file header or the method
  (`📝 2026-11-05 20:27 · text`, newest first, at most 3), `--body` above the method source. `note list [<path>]`
  lists them (with `[orphaned]` when the method is gone), `note rm <id>` deletes one, and
  `show <dir> --notes <pattern>` searches them. The anchor is the method name, not a line number. MCP: a new
  read-only `notes` tool (there is no write tool). Without notes the output is unchanged. Details: `docs/NOTES.md`.
- **TypeScript: calls through typed objects and interfaces.** callgraph now works out which class a TS method call
  goes to when the type is written down: a typed parameter (`repo: Repo`), a typed class property
  (`private repo: Repo`, also `constructor(private repo: Repo)`), `this.repo = new Repo()` or `const x = new Repo()`.
  So `this.repo.find()` shows up as `Repo::find`, and `--grep 'Repo::find'` finds it. Classes record their
  `implements`, and TS interfaces (`interface X extends Y`) appear in the map, so searching
  `PlaylistResolver::getPlayables` also finds calls made through its `Resolver` interface, marked `via Resolver`
  (in `--depth` and `callers` too). When the type is unclear (no type, `any`, a union like `Repo | null`, a generic
  `T`, or different values in the same function) the call stays as before.
- **`--const` lists re-exports:** besides references and definitions, `--const NAME` now shows where a name is passed
  on, marked `[re-export]`: Rust `pub use state::{CheckState, STATE_FILE};` / `pub(crate) use …`, JS / TS
  `export { NAME } from './x'` / `export { NAME }`, PHP `use const App\NAME;`.

### Fixed
- `--grep <pattern> --body` prints the source of the caller methods again (the option was ignored).
- MCP `method_body`: `method` may be a single name as a plain string as well as a list (some clients send
  `"method": "parse"`, which used to fail with an "expected a sequence" error).

### Fixed
- **JS/TS: no more doubled methods when a class is named like its file** (`PlaylistResolver.ts` with
  `class PlaylistResolver`): the class's methods were listed twice, once under the file's `module` entry and once
  under the class. Now class members appear only under the class and top-level functions only under the module.

## [0.2.0] — 2026-11-05

### Added
- **Background update check:** at most once a day `callgraph.sh` checks GitHub in a detached background process
  (never slows a query down). If a newer release exists, the output ends with one line, once a day:
  `ℹ callgraph X is available (installed: Y) — update: bash …/callgraph.sh update`. Claude Code tells the user and
  offers the update; it never updates on its own. Opt out with `CALLGRAPH_UPDATE_CHECK=0`; off in CI.
- **Bug reporting via GitHub:** bilingual issue forms (bug report, idea / feedback), a private security report link and
  `SECURITY.md`. Error messages now point to the bug form; `feedback.sh show <error-id>` prints the details to paste.
- **English skill instructions:** `SKILL.md` now exists in English and Hungarian (`skill/skill-i18n/SKILL.<en|hu>.md`).
  The skill follows the language: `callgraph.sh set-lang <en|hu>` rewrites `SKILL.md` in that language (the other one
  disappears); the first install (`install.sh` → new `callgraph skill-sync`) and `update` keep the user's language.
  Restart Claude Code to reload the skill.
- **Release checksums:** every release has a `SHA256SUMS` asset (`sha256sum` format, all assets). `update` verifies the
  downloaded zip's SHA-256 before installing and installs nothing on a mismatch; a release without `SHA256SUMS` (v0.1.0)
  is refused unless `--insecure-skip-checksum` is given. `update --from <zip>` checks `--sha256 <hex>` or a
  `SHA256SUMS` next to the zip, otherwise it warns in one line and installs.
- **MCP server mode:** `callgraph mcp` runs a Model Context Protocol server on stdio (official Rust SDK `rmcp`), so
  Claude Desktop, Cursor, VS Code or any MCP client can use callgraph. Tools: `outline`, `method_body`, `find_callers`,
  `search`, `framework_view`, `projects` — the same compact text and output cap as the CLI. Read-only, no network,
  paths confined to the project root. The skill / CLI usage is unchanged. Setup recipes: `docs/MCP.md`.
- **See how something is called:** "where is it called" results (`--grep`, and every level of `--depth`) now show the
  call's own source line next to its line number, e.g. `L25 │ $this->repo->find($id)` — no extra `--body` call needed
  just to see the arguments.
- **Calls through interfaces and traits:** searching a class method (`--grep 'PlaylistResolver::getPlayables'`) also
  finds the calls made through its interface, abstract parent or trait (and, the other way round, through subclasses
  that don't override it). These are marked, e.g. `L14 via Resolver │ …`; `--depth` follows them to the next level, and
  `callers` adds a `"via"` field. Works where the call's receiver type is known (PHP: typed properties and other
  resolved receivers; Rust: `Type::method` calls); JS/TS interfaces and Swift protocols are not tracked yet.
- **Rust: smarter method calls.** For `x.load()` callgraph now works out the type of `x` in the simple cases
  (parameter type, `let x = Repo::new()`, `Repo { … }`, `self`, a field of your own struct like `self.repo`), so the
  call shows up as `Repo::load` and "who calls `Repo::load`?" finds it. When the type can't be worked out, the call is
  kept as `?::load` and a `Repo::load` search still lists it, marked as uncertain — instead of silently missing it.
- **Rust constants:** `--const` now finds Rust `const` / `static` items — where they are defined and where they are
  used (also `Type::CONST`).
- **JS / TS object methods:** functions inside an object literal (`const api = { fetch() {}, save: () => {} }`, or a
  plain `export default { … }`) are listed as separate methods (`api.fetch`) with their line ranges, and calls to them
  are found.

### Changed
- **Faster builds:** `tools/dev.sh check` / `test` / `clippy` use the debug profile (like CI now; `check-release` keeps
  the LTO variant), the dev container shares one build cache across all checkouts (`CALLGRAPH_DEV_TARGET`), and CI
  caches the compiled dependencies (`Swatinem/rust-cache`) and runs clippy + tests without LTO. The released binaries
  are built exactly as before (release profile, LTO).
- **Shorter result lists:** when the project root would repeat on many lines (`--grep`, `--depth`, `--strings`,
  `--const`, folder outlines, framework views…), the first line is now `root: /abs/path/to/project` and every path after
  it is relative to that root. Outputs with a single path, and the machine formats (`json`, `mermaid`, `dot`, `callers`),
  are unchanged.
- The committed / packaged default `SKILL.md` is now English (generated from `skill-i18n/SKILL.en.md`).
- `SKILL.md` now recommends finding callers by name (`--grep '(^|::|\.|->)name$'`), which also catches inherited,
  trait and unknown-receiver calls; `Type::method` is for narrowing a common name. It also explains the new search
  output (`root:` line, `L<n> │ code`, `via`, `?::`) and the new Rust / JS options.
- `--const` also lists the matching constant definitions (marked `[definition]`), not only the references.
- Version lookup for `update` / `update --check` uses the `releases/latest` redirect instead of the GitHub API, so the
  API rate limit (HTTP 403) no longer applies.
- The project page moved to the `gh-pages` branch (https://geriy.github.io/callgraph/); page edits no longer touch `main`.

### Fixed
- Windows: the project root is printed as `C:\…` instead of `\\?\C:\…` in search output.
- A bare method selector also finds an object-literal method: `store.ts::updateSongs` selects `playableStore.updateSongs`.
- `--grep … --depth`: from level 2 on, callers are searched in the whole project (from the project root), not only in
  the folders of the level-1 matches — callers in other folders were missed.
- Rust `--format api` now lists the `pub fn` methods of `impl` blocks (and trait implementations), the `pub` constants
  and fields — before, it showed no public methods for Rust types.
- `--strings` now finds texts inside Vue template expressions (`{{ … }}`, `:title="…"`, `@click="…"`) and in Rust
  `const` / `static` values, including byte strings (`b"…"`).
- SCSS: `@mixin` / `@function` parameters (`$color: t.$brand, …`) and `@include` keyword arguments were listed as
  constants; now only `$var: value;` declarations are (the parameters stay the mixin's params).
- `update --check` now also records its result in `update-check.json`, so the daily background check does not repeat
  it and a newer version found manually shows up in the notice.

## [0.1.0] — 2026-11-05

First release.

- Languages: PHP, JavaScript / TypeScript, Vue, **Rust**, **Swift / SwiftUI**, CSS / SCSS, JSON, Markdown.
- Structure, categorized call graph, method bodies, "who calls this?" and impact analysis (`--grep --depth`),
  framework views (Laravel / Yii), many output formats.
- English or Hungarian output (`callgraph.sh set-lang en|hu`).
- Built-in update: `callgraph --version`, `callgraph.sh update [--check|--force]`.
- Prebuilt packages for macOS (arm64, x86_64), Linux (x86_64, ARM64, static musl) and Windows.

[Unreleased]: https://github.com/GeRiY/callgraph/compare/v0.5.3...develop
[0.5.3]: https://github.com/GeRiY/callgraph/releases/tag/v0.5.3
[0.5.2]: https://github.com/GeRiY/callgraph/releases/tag/v0.5.2
[0.5.1]: https://github.com/GeRiY/callgraph/releases/tag/v0.5.1
[0.5.0]: https://github.com/GeRiY/callgraph/releases/tag/v0.5.0
[0.4.0]: https://github.com/GeRiY/callgraph/releases/tag/v0.4.0
[0.3.0]: https://github.com/GeRiY/callgraph/releases/tag/v0.3.0
[0.2.0]: https://github.com/GeRiY/callgraph/releases/tag/v0.2.0
[0.1.0]: https://github.com/GeRiY/callgraph/releases/tag/v0.1.0
