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
- **MCP server mode:** `callgraph mcp` runs a Model Context Protocol server on stdio (official Rust SDK `rmcp`), so
  Claude Desktop, Cursor, VS Code or any MCP client can use callgraph. Tools: `outline`, `method_body`, `find_callers`,
  `search`, `framework_view`, `projects` — the same compact text and output cap as the CLI. Read-only, no network,
  paths confined to the project root. The skill / CLI usage is unchanged. Setup recipes: `docs/MCP.md`.

### Changed
- Version lookup for `update` / `update --check` uses the `releases/latest` redirect instead of the GitHub API, so the
  API rate limit (HTTP 403) no longer applies.
- The project page moved to the `gh-pages` branch (https://geriy.github.io/callgraph/); page edits no longer touch `main`.

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
