#!/usr/bin/env python3
"""Checks the generated site (run after tools/build.py). Exit code 1 on any error.

Per page: unique title / description within length limits, exactly one <h1>, canonical = own URL,
reciprocal hreflang (en, hu, x-default), every internal link / asset resolves to a file and, with a
#fragment, to an existing id, JSON-LD parses (BreadcrumbList on every indexable page, FAQ questions
visible on the page), a markdown alternate exists. Sitemap lists exactly the HTML pages. Feed and
llms.txt exist and parse.
"""
import json
import os
import re
import sys
import xml.etree.ElementTree as ET
from html.parser import HTMLParser
from urllib.parse import urljoin, urlparse

ROOT = os.path.abspath(sys.argv[1]) if len(sys.argv) > 1 else sys.exit("usage: python3 site/check.py <gh-pages checkout>")
# user documentation only: internal / developer terms must not appear on any served page
DEV_TERMS = re.compile(r"dev\.sh|\bclippy\b|\bLTO\b|rust-cache|Docker|gh-pages|skill-i18n|debug profile", re.I)
BASE = "https://callgraph.eu"
TITLE_MAX, DESC_MAX, DESC_MIN = 60, 155, 135
SKIP_DIRS = {".git", "tools", "assets"}

errors, warnings = [], []


class Page(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.title = None
        self._in_title = False
        self.meta = {}
        self.links = []          # (rel, hreflang, href, type)
        self.anchors = []        # href of <a>
        self.srcs = []
        self.ids = set()
        self.h1 = 0
        self.lang = None
        self.jsonld = []
        self._in_ld = False
        self._buf = ""
        self.text = []
        self.summaries = []
        self._in_summary = False
        self.imgs = []

    BALANCED = ("section", "div", "ul", "ol", "table", "details", "nav", "main", "pre", "a", "h1", "h2", "h3")

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if tag in self.BALANCED:
            self.balance = getattr(self, "balance", {})
            self.balance[tag] = self.balance.get(tag, 0) + 1
        if "id" in a:
            if a["id"] in self.ids:
                errors.append("duplicate id %r" % a["id"])
            self.ids.add(a["id"])
        if tag == "html":
            self.lang = a.get("lang")
        elif tag == "title":
            self._in_title = True
            self.title = ""
        elif tag == "meta":
            k = a.get("name") or a.get("property")
            if k:
                self.meta[k] = a.get("content", "")
        elif tag == "link":
            self.links.append((a.get("rel"), a.get("hreflang"), a.get("href"), a.get("type")))
        elif tag == "a":
            self.anchors.append(a.get("href", ""))
        elif tag in ("img", "script") and a.get("src"):
            self.srcs.append(a["src"])
        if tag == "img":
            self.imgs.append(a)
        if tag == "h1":
            self.h1 += 1
        if tag == "script" and a.get("type") == "application/ld+json":
            self._in_ld = True
            self._buf = ""
        if tag == "summary":
            self._in_summary = True
            self._sbuf = ""

    def handle_endtag(self, tag):
        if tag in self.BALANCED:
            self.balance = getattr(self, "balance", {})
            self.balance[tag] = self.balance.get(tag, 0) - 1
        if tag == "title":
            self._in_title = False
        if tag == "script" and self._in_ld:
            self._in_ld = False
            self.jsonld.append(self._buf)
        if tag == "summary" and self._in_summary:
            self._in_summary = False
            self.summaries.append(self._sbuf.strip())

    def handle_data(self, d):
        if self._in_title:
            self.title += d
        if self._in_ld:
            self._buf += d
        else:
            self.text.append(d)
        if self._in_summary:
            self._sbuf += d


def html_files():
    for dp, dns, fns in os.walk(ROOT):
        dns[:] = [d for d in dns if d not in SKIP_DIRS]
        for f in fns:
            if f.endswith(".html"):
                yield os.path.relpath(os.path.join(dp, f), ROOT)


def url_of(rel):
    if rel == "404.html":
        return BASE + "/404.html"
    d = os.path.dirname(rel)
    return BASE + "/" + (d + "/" if d else "")


def target_file(u):
    """Map an internal absolute URL to a file path (or None)."""
    p = urlparse(u).path or "/"
    if p.endswith("/"):
        p += "index.html"
    return os.path.join(ROOT, p.lstrip("/"))


def main():
    pages = {}
    for rel in sorted(html_files()):
        p = Page()
        p.feed(open(os.path.join(ROOT, rel), encoding="utf-8").read())
        p.close()
        pages[rel] = p
    ids_by_file = {os.path.join(ROOT, rel): p.ids for rel, p in pages.items()}
    titles, descs = {}, {}
    indexable = {}
    for rel, p in pages.items():
        me = url_of(rel)
        where = rel
        for tag, n in getattr(p, "balance", {}).items():
            if n:
                errors.append("%s: <%s> open/close mismatch (%+d)" % (where, tag, n))
        if p.h1 != 1:
            errors.append("%s: %d <h1>" % (where, p.h1))
        if not p.title:
            errors.append("%s: no <title>" % where)
        noindex = "noindex" in p.meta.get("robots", "")
        if p.lang not in ("en", "hu"):
            errors.append("%s: bad html lang %r" % (where, p.lang))
        for img in p.imgs:
            if not (img.get("width") and img.get("height") and "alt" in img):
                errors.append("%s: <img %s> without width/height/alt" % (where, img.get("src")))
        # links and assets
        for href in p.anchors + [l[2] for l in p.links if l[0] in ("stylesheet", "icon", "alternate")] + p.srcs:
            if not href or href.startswith(("mailto:", "javascript:")):
                continue
            absu = urljoin(me, href)
            if not absu.startswith(BASE):
                continue
            f = target_file(absu)
            if not os.path.isfile(f):
                errors.append("%s: broken link %s" % (where, href))
                continue
            frag = urlparse(absu).fragment
            if frag and f.endswith(".html") and frag not in ids_by_file.get(f, set()):
                errors.append("%s: missing anchor %s" % (where, href))
        # JSON-LD
        lds = []
        for raw in p.jsonld:
            try:
                lds.append(json.loads(raw))
            except ValueError as e:
                errors.append("%s: JSON-LD does not parse: %s" % (where, e))
        if noindex:
            continue
        indexable[me] = (rel, p)
        t, d = p.title, p.meta.get("description", "")
        if len(t) > TITLE_MAX:
            errors.append("%s: title %d > %d chars: %s" % (where, len(t), TITLE_MAX, t))
        if not d:
            errors.append("%s: no meta description" % where)
        elif len(d) > DESC_MAX:
            errors.append("%s: description %d > %d chars" % (where, len(d), DESC_MAX))
        elif len(d) < DESC_MIN:
            warnings.append("%s: description only %d chars" % (where, len(d)))
        if t in titles:
            errors.append("%s: title duplicates %s" % (where, titles[t]))
        titles[t] = where
        if d in descs:
            errors.append("%s: description duplicates %s" % (where, descs[d]))
        descs[d] = where
        canon = [l[2] for l in p.links if l[0] == "canonical"]
        if canon != [me]:
            errors.append("%s: canonical %s != %s" % (where, canon, me))
        if p.meta.get("og:url") != me:
            errors.append("%s: og:url %s != %s" % (where, p.meta.get("og:url"), me))
        alts = {l[1]: l[2] for l in p.links if l[0] == "alternate" and l[1]}
        if set(alts) != {"en", "hu", "x-default"}:
            errors.append("%s: hreflang set %s" % (where, sorted(alts)))
        elif alts[p.lang] != me:
            errors.append("%s: hreflang %s does not point to itself" % (where, p.lang))
        elif alts["x-default"] != alts["en"]:
            errors.append("%s: x-default != en" % where)
        p.alts = alts
        md = [l[2] for l in p.links if l[0] == "alternate" and l[3] == "text/markdown"]
        if len(md) != 1:
            errors.append("%s: expected one markdown alternate, got %s" % (where, md))
        types = [o.get("@type") for o in lds]
        bc = [o for o in lds if o.get("@type") == "BreadcrumbList"]
        if len(bc) != 1:
            errors.append("%s: expected one BreadcrumbList, got %s" % (where, types))
        elif bc[0]["itemListElement"][-1]["item"] != me:
            errors.append("%s: last breadcrumb item != canonical" % where)
        for o in lds:
            if o.get("@type") == "FAQPage":
                for q in o["mainEntity"]:
                    if q["name"] not in p.summaries:
                        errors.append("%s: FAQ question not visible: %s" % (where, q["name"]))
        p.ldtypes = types
    # hreflang reciprocity
    for me, (rel, p) in indexable.items():
        for lang, u in getattr(p, "alts", {}).items():
            if lang == "x-default":
                continue
            if u not in indexable:
                errors.append("%s: hreflang %s -> %s is not a page" % (rel, lang, u))
                continue
            back = getattr(indexable[u][1], "alts", {}).get(p.lang)
            if back != me:
                errors.append("%s: hreflang %s -> %s does not link back (%s)" % (rel, lang, u, back))
    # sitemap
    ns = {"s": "http://www.sitemaps.org/schemas/sitemap/0.9", "x": "http://www.w3.org/1999/xhtml"}
    tree = ET.parse(os.path.join(ROOT, "sitemap.xml"))
    locs = {}
    for u in tree.getroot().findall("s:url", ns):
        loc = u.find("s:loc", ns).text
        locs[loc] = {l.get("hreflang"): l.get("href") for l in u.findall("x:link", ns)}
        if u.find("s:lastmod", ns) is None:
            errors.append("sitemap: %s without lastmod" % loc)
    if set(locs) != set(indexable):
        errors.append("sitemap mismatch: missing %s, extra %s" % (sorted(set(indexable) - set(locs)),
                                                               sorted(set(locs) - set(indexable))))
    for loc, alts in locs.items():
        if loc in indexable and alts != getattr(indexable[loc][1], "alts", None):
            errors.append("sitemap: alternates of %s differ from the page" % loc)
    # feed, llms.txt, markdown
    try:
        feed = ET.parse(os.path.join(ROOT, "changelog", "feed.xml")).getroot()
        entries = feed.findall("{http://www.w3.org/2005/Atom}entry")
        titles_f = [e.find("{http://www.w3.org/2005/Atom}title").text for e in entries]
        if not entries or any("nreleased" in t for t in titles_f):
            errors.append("feed: bad entries %s" % titles_f)
    except (ET.ParseError, OSError) as e:
        errors.append("feed: %s" % e)
        titles_f = []
    for f in ("llms.txt", "hu/llms.txt"):
        path = os.path.join(ROOT, f)
        if not os.path.isfile(path):
            errors.append("%s missing" % f)
            continue
        for u in re.findall(r"\]\((%s[^)]*)\)" % re.escape(BASE), open(path, encoding="utf-8").read()):
            if not os.path.isfile(target_file(u)):
                errors.append("%s: broken link %s" % (f, u))
    for rel, p in pages.items():
        if "Unreleased" in "".join(p.text) and "changelog" in rel:
            errors.append("%s: shows Unreleased" % rel)
    # user documentation only: no developer files are served, and no internal terms on any served text file
    for name in ("tools", "README.md", "site"):
        if os.path.exists(os.path.join(ROOT, name)):
            errors.append("%s must not be published (developer material)" % name)
    for dp, dns, fns in os.walk(ROOT):
        dns[:] = [d for d in dns if d != ".git"]
        for f in fns:
            if f.endswith((".html", ".md", ".txt", ".xml")):
                rel = os.path.relpath(os.path.join(dp, f), ROOT)
                found = DEV_TERMS.findall(open(os.path.join(dp, f), encoding="utf-8").read())
                if found:
                    errors.append("%s: developer term(s) %s" % (rel, sorted(set(found))))
    print("checked %d HTML files (%d indexable), %d sitemap URLs, %d feed entries"
          % (len(pages), len(indexable), len(locs), len(titles_f)))
    for me, (rel, p) in sorted(indexable.items()):
        print("  %-26s title %2d / desc %3d  %s" % (rel, len(p.title), len(p.meta.get("description", "")), p.title))
    for w in warnings:
        print("warning: " + w)
    for e in errors:
        print("ERROR: " + e)
    print("OK" if not errors else "%d error(s)" % len(errors))
    sys.exit(1 if errors else 0)


if __name__ == "__main__":
    main()
