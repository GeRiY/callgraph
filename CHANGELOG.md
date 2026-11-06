# Changelog

All notable changes to callgraph. Work is collected in packages on the `develop` branch and released together.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versions: [SemVer](https://semver.org/).

## [Unreleased]

### Changed
- The project website has its own domain: https://callgraph.eu (the old `geriy.github.io/callgraph` address redirects).

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

[Unreleased]: https://github.com/GeRiY/callgraph/compare/v0.3.0...develop
[0.3.0]: https://github.com/GeRiY/callgraph/releases/tag/v0.3.0
[0.2.0]: https://github.com/GeRiY/callgraph/releases/tag/v0.2.0
[0.1.0]: https://github.com/GeRiY/callgraph/releases/tag/v0.1.0
