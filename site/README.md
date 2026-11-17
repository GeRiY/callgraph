# callgraph.eu — the website generator

The public website (https://callgraph.eu) is served by GitHub Pages from the `gh-pages` branch. That branch holds only
the generated pages: user documentation, nothing developer-facing. The generator and the page sources live here.

```bash
# the gh-pages branch checked out somewhere, e.g. a worktree
python3 site/build.py <gh-pages checkout>   # writes every page, sitemap.xml, llms.txt, the changelog feed
python3 site/check.py <gh-pages checkout>   # SEO / link / hreflang / JSON-LD checks + "no developer material"
```

- Page text: `site/content/{en,hu}/<page>.html`; titles, descriptions, navigation, FAQ and HowTo data: `site/build.py`.
- The changelog page is built from the repository's `CHANGELOG.md` (English) and `CHANGELOG.hu.md` (Hungarian, same
  releases and sections — every release goes into both; the build stops if their versions differ): released versions only (no Unreleased), and only
  user-facing entries — entries about the project's own build, CI, packaging or repository are filtered out
  (`DEV_ENTRY` in `build.py`). The site version (download button, release badge, `softwareVersion`) is the newest
  released version there.
- After a release: the live `gh-pages` also has pages the generator does not produce yet (laravel, gen, go, python and
  their menu entries), so do **not** run a full `build.py` over it. Use the changelog-only mode, which touches nothing
  but the changelog and the version strings:

  ```bash
  python3 site/build.py --changelog-only <gh-pages checkout>
  python3 site/check.py <gh-pages checkout>
  ```

  It adds the missing release sections to the changelog pages (en/hu html + md) and the Atom feed, refreshes the
  release index, and replaces the old version in the home pages and `llms.txt` (badge, download button,
  `softwareVersion`). It is idempotent: sections already present are left alone. Then take screenshots of visible
  changes for approval, commit and push the `gh-pages` branch. (`tools/release.sh` prints this as a follow-up.)
