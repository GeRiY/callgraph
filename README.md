# callgraph

Token-hatékony kód-térkép agenteknek (Claude Code skill): PHP, JS / TS / Vue, CSS / SCSS, JSON és Markdown fájlok
szerkezete, kategorizált hívás-gráfja, metódus-törzse, „ki hívja” keresése és hatáselemzése — a fájl beolvasása nélkül.
Az elemző egy önálló Rust-bináris (`callgraph`); a `skill/callgraph.sh` vékony burok fölötte.

- Használat és telepítés: [`skill/SKILL.md`](skill/SKILL.md), [`skill/README.md`](skill/README.md)
- Felépítés: [`ARCHITECTURE.md`](ARCHITECTURE.md), a lekérdező réteg: [`docs/QUERY.md`](docs/QUERY.md), új nyelv: [`docs/UJ-NYELV.md`](docs/UJ-NYELV.md)

## Build

Fejlesztés Dockerben (helyben csak Docker kell; a Rust toolchain, a clippy, a rustfmt, a python3 és a php a konténerben van,
az image a `docker/dev.Dockerfile`):

```bash
bash tools/dev.sh build        # target/docker/release/callgraph
bash tools/dev.sh test
bash tools/dev.sh check        # fmt --check + clippy -D warnings + test (commit előtt)
bash tools/dev.sh skill        # dist/callgraph-<verzió>.tar.gz (a skill-csomag, Linux)
bash tools/dev.sh shell        # interaktív shell a konténerben
```

Helyi toolchainnel ugyanez: `cargo build --release`, `cargo test --release`, `bash tools/build-skill.sh`.

A több platformos csomagot (macOS arm64 / x86_64, Linux arm64 / x86_64 musl, Windows x86_64) a GitHub Actions
készíti (`.github/workflows/release.yml`); `v*` tagnél GitHub Release-be teszi.

## Licenc

MIT — lásd [`LICENSE`](LICENSE).
