# callgraph

**English** | [Magyar](README.hu.md)

A token-efficient code map for agents (a Claude Code skill). For PHP, JS / TS / Vue, Swift / SwiftUI, CSS / SCSS, JSON and Markdown
files it gives you the structure, a categorized call graph, method bodies, "who calls this" search and impact
analysis, without reading the file. The analyzer is a standalone Rust binary (`callgraph`); `skill/callgraph.sh` is a
thin wrapper on top of it.

- Usage and installation: [`skill/SKILL.md`](skill/SKILL.md), [`skill/README.md`](skill/README.md)
- Design: [`ARCHITECTURE.md`](ARCHITECTURE.md), the query layer: [`docs/QUERY.md`](docs/QUERY.md), adding a language: [`docs/UJ-NYELV.md`](docs/UJ-NYELV.md)
  (the detailed docs are currently in Hungarian)

## Download and install

The ready-to-install package (skill files, the binary for your machine, install guide) is available from the latest
[GitHub Release](https://github.com/GeRiY/callgraph/releases/latest):

| Machine | Download |
|---|---|
| Mac, Apple Silicon (M1–M4) | [callgraph-darwin-arm64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-darwin-arm64.zip) |
| Mac, Intel | [callgraph-darwin-x86_64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-darwin-x86_64.zip) |
| Linux x86_64 | [callgraph-linux-x86_64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-linux-x86_64.zip) |
| Linux ARM64 | [callgraph-linux-aarch64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-linux-aarch64.zip) |
| Windows x86_64 (Git Bash) | [callgraph-windows-x86_64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-windows-x86_64.zip) |

All five binaries in one package: the Release's `callgraph-<version>.tar.gz` file.

**Quick install (macOS / Linux).** This downloads the package for your machine into `~/.claude/skills/callgraph/` and
runs `install.sh`. On update, `projects.json`, `errors.jsonl` and `feedback.jsonl` are kept.

```bash
p="$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)" && t="$(mktemp -d)" \
  && curl -fsSL -o "$t/cg.zip" "https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-$p.zip" \
  && mkdir -p ~/.claude/skills && unzip -qo "$t/cg.zip" -d ~/.claude/skills \
  && bash ~/.claude/skills/callgraph/install.sh && rm -rf "$t"
```

**Manually (also on Windows):** unzip the package, copy the `callgraph/` folder inside it under `~/.claude/skills/`
(Windows: `%USERPROFILE%\.claude\skills\`), then run `bash install.sh` there. Details (in Hungarian): [`skill/TELEPITES.md`](skill/TELEPITES.md).

Then register your project (`bash ~/.claude/skills/callgraph/projects.sh add myapp /absolute/path/to/project`) and
restart Claude Code.

## Build

Develop in Docker (only Docker is needed locally; the Rust toolchain, clippy, rustfmt, python3 and php are in the
container, the image is `docker/dev.Dockerfile`):

```bash
bash tools/dev.sh build        # target/docker/release/callgraph
bash tools/dev.sh test
bash tools/dev.sh check        # fmt --check + clippy -D warnings + test (before committing)
bash tools/dev.sh skill        # dist/callgraph-<version>.tar.gz (the skill package, Linux)
bash tools/dev.sh shell        # interactive shell in the container
```

With a local toolchain: `cargo build --release`, `cargo test --release`, `bash tools/build-skill.sh`.

The multi-platform package (macOS arm64 / x86_64, Linux arm64 / x86_64 musl, Windows x86_64) is built by GitHub
Actions (`.github/workflows/release.yml`); on a `v*` tag it is published as a GitHub Release.

## License

MIT — see [`LICENSE`](LICENSE).
