# Changelog

All notable changes to callgraph. Work is collected in packages on the `develop` branch and released together.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versions: [SemVer](https://semver.org/).

## [Unreleased] — planned as 0.2.0

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
- **Shorter result lists:** when the project root would repeat on many lines (`--grep`, `--depth`, `--strings`,
  `--const`, folder outlines, framework views…), the first line is now `root: /abs/path/to/project` and every path after
  it is relative to that root. Outputs with a single path, and the machine formats (`json`, `mermaid`, `dot`, `callers`),
  are unchanged.
- The committed / packaged default `SKILL.md` is now English (generated from `skill-i18n/SKILL.en.md`).
- `--const` also lists the matching constant definitions (marked `[definition]`), not only the references.
- Version lookup for `update` / `update --check` uses the `releases/latest` redirect instead of the GitHub API, so the
  API rate limit (HTTP 403) no longer applies.
- The project page moved to the `gh-pages` branch (https://geriy.github.io/callgraph/); page edits no longer touch `main`.

### Fixed
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

[Unreleased]: https://github.com/GeRiY/callgraph/compare/v0.1.0...develop
[0.1.0]: https://github.com/GeRiY/callgraph/releases/tag/v0.1.0
