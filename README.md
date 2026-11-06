# callgraph — project page

This branch only holds the static project page published at <https://callgraph.eu/>
(GitHub Pages: *Deploy from a branch* → `gh-pages` / `(root)`). The tool itself lives on [`main`](https://github.com/GeRiY/callgraph).

## Rebuild

The HTML is generated (Python 3, standard library only) and the generated files are committed:

```bash
python3 tools/build.py    # writes */index.html, */index.md, 404.html, sitemap.xml, llms.txt, changelog/feed.xml
python3 tools/check.py    # titles, descriptions, one h1, canonical, hreflang, links, anchors, JSON-LD, sitemap
```

- Page text: `tools/site/en/<page>.html` and `tools/site/hu/<page>.html` (the inside of `<main>`; a fenced
  ```` ``` ```` block becomes an escaped code block, ```` ```out ```` an example-output block).
- Titles, descriptions, navigation, FAQ and structured data: `tools/build.py` (`PAGES`, `UI`, `FAQ`, `HOWTO`).
- Changelog page, feed and the version shown on the site come from `tools/site/CHANGELOG.md` (the `Unreleased`
  section is skipped). After a release refresh it from develop, then rebuild:
  `git show origin/develop:CHANGELOG.md > tools/site/CHANGELOG.md`
- `LASTMOD=YYYY-MM-DD python3 tools/build.py` overrides the sitemap date (default: the newest release date).
- Do not edit the generated HTML by hand; `assets/`, `CNAME`, `.nojekyll` and `robots.txt` are not generated.
