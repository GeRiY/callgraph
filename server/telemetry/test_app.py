"""Tests for the telemetry receiver: python3 -I server/telemetry/test_app.py"""

import json
import os
import sqlite3
import sys
import tempfile
import threading
import unittest
import urllib.error
import urllib.request
from datetime import date, timedelta
from http.server import ThreadingHTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))
TMP = tempfile.mkdtemp()
os.environ["TELEMETRY_DB"] = os.path.join(TMP, "t.sqlite")
sys.path.insert(0, HERE)
import app  # noqa: E402
import counts  # noqa: E402

app.DB_PATH = os.environ["TELEMETRY_DB"]


def payload(day=None, batch="4aa9afdc-dd5e-484d-8885-41aa8f9a075e", commands=None):
    day = day or (date.today() - timedelta(days=1)).isoformat()
    commands = commands or {"show": {"calls": 4, "clients": {"cli": 4}, "latency": [4, 0, 0, 0, 0], "size": [4, 0, 0, 0]}}
    return {"schema": 1, "version": "0.6.0", "platform": "linux-x86_64",
            "days": [{"day": day, "batch": batch, "commands": commands}]}


class Receiver(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        app.db().close()
        cls.srv = ThreadingHTTPServer(("127.0.0.1", 0), app.Handler)
        cls.url = "http://127.0.0.1:%d/v1" % cls.srv.server_address[1]
        threading.Thread(target=cls.srv.serve_forever, daemon=True).start()

    @classmethod
    def tearDownClass(cls):
        cls.srv.shutdown()

    def post(self, body, raw=False):
        data = body if raw else json.dumps(body).encode()
        req = urllib.request.Request(self.url, data=data, headers={"Content-Type": "application/json"})
        try:
            return urllib.request.urlopen(req, timeout=5).status
        except urllib.error.HTTPError as e:
            return e.code

    def rows(self):
        con = sqlite3.connect(app.DB_PATH)
        try:
            return con.execute("SELECT batch, day, commands FROM days").fetchall()
        finally:
            con.close()

    def test_accepts_and_dedupes(self):
        before = len(self.rows())
        self.assertEqual(self.post(payload(batch="aaaa-1")), 204)
        self.assertEqual(self.post(payload(batch="aaaa-1")), 204)
        self.assertEqual(len(self.rows()), before + 1)

    def test_rejects_paths_names_and_bad_shapes(self):
        bad = [
            payload(commands={"show": {"calls": 1, "file": "app/Order.php"}}),
            payload(commands={"/home/x": {"calls": 1}}),
            payload(commands={"show": {"calls": -1}}),
            payload(commands={"show": {"calls": True}}),
            payload(commands={"show": {"calls": app.MAX_COUNT + 1}}),
            payload(commands={"show": {"latency": [0, app.MAX_COUNT + 1]}}),
            payload(day=(date.today() + timedelta(days=3)).isoformat()),
            payload(day="2020-01-01"),
            payload(batch="not a uuid!"),
            {"schema": 2, "version": "x", "platform": "y", "days": []},
            {**payload(), "extra": {"days": []}, "days": []},
        ]
        for body in bad:
            self.assertEqual(self.post(body), 422, body)
        self.assertEqual(self.post(b"{not json", raw=True), 400)
        self.assertEqual(self.post(b"x" * (app.MAX_BODY + 1), raw=True), 413)

    def test_no_addresses_or_headers_stored(self):
        self.post(payload(batch="bbbb-2"))
        con = sqlite3.connect(app.DB_PATH)
        cols = [r[1] for r in con.execute("PRAGMA table_info(days)")]
        con.close()
        self.assertEqual(cols, ["batch", "day", "version", "platform", "commands", "received"])

    def test_old_days_fold_into_months(self):
        con = app.db()
        old = (date.today() - timedelta(days=app.RETENTION_DAYS + 5)).isoformat()
        for b in ("cccc-1", "cccc-2"):
            con.execute("INSERT INTO days VALUES (?,?,?,?,?,?)",
                        (b, old, "0.6.0", "linux-x86_64", json.dumps({"show": {"calls": 2, "latency": [2, 0]}}), old))
        con.commit()
        app.fold_old_days(con)
        self.assertEqual(con.execute("SELECT COUNT(*) FROM days WHERE day=?", (old,)).fetchone()[0], 0)
        batches, commands = con.execute("SELECT batches, commands FROM months WHERE month=?", (old[:7],)).fetchone()
        con.close()
        self.assertEqual(batches, 2)
        self.assertEqual(json.loads(commands)["show"]["calls"], 4)

    def hit(self, body):
        req = urllib.request.Request(self.url.replace("/v1", "/hit"), data=json.dumps(body).encode(),
                                     headers={"Content-Type": "text/plain"})
        try:
            return urllib.request.urlopen(req, timeout=5).status
        except urllib.error.HTTPError as e:
            return e.code

    def test_page_hits_are_counted_without_anything_personal(self):
        self.assertEqual(self.hit({"p": "/hu/changelog/", "r": "www.linkedin.com", "e": "view"}), 204)
        self.assertEqual(self.hit({"p": "/hu/changelog/", "r": "linkedin.com", "e": "view"}), 204)
        self.assertEqual(self.hit({"p": "/", "r": "", "e": "download"}), 204)
        for bad in ({"p": "https://x.y/", "e": "view"}, {"p": "/a", "e": "click"}, {"p": "/a", "r": "a b", "e": "view"},
                    {"p": "/../etc", "e": "view"}, {"p": "/a", "e": "view", "ip": "1.2.3.4"}):
            self.assertEqual(self.hit(bad), 422, bad)
        con = sqlite3.connect(app.DB_PATH)
        rows = con.execute("SELECT page, referrer, event, count FROM hits ORDER BY page").fetchall()
        cols = [r[1] for r in con.execute("PRAGMA table_info(hits)")]
        con.close()
        self.assertIn(("/hu/changelog/", "linkedin.com", "view", 2), rows)
        self.assertIn(("/", "", "download", 1), rows)
        self.assertEqual(cols, ["day", "page", "referrer", "event", "count"])

    def test_full_database_refuses_new_data(self):
        old = app.MAX_DB_BYTES
        app.MAX_DB_BYTES = 1
        try:
            self.assertEqual(self.post(payload(batch="eeee-1")), 507)
            self.assertEqual(self.hit({"p": "/", "e": "view"}), 507)
        finally:
            app.MAX_DB_BYTES = old

    def test_rate_limit(self):
        app._hits.clear()
        codes = [self.post(payload(batch="dddd-%d" % i)) for i in range(app.RATE_LIMIT + 2)]
        self.assertIn(429, codes)
        app._hits.clear()


class _NoFollow(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


class Counting(unittest.TestCase):
    """check.callgraph.eu / update.callgraph.eu: anonymous daily counters and the GitHub redirects."""

    @classmethod
    def setUpClass(cls):
        app.db().close()
        cls.srv = ThreadingHTTPServer(("127.0.0.1", 0), app.Handler)
        cls.base = "http://127.0.0.1:%d" % cls.srv.server_address[1]
        threading.Thread(target=cls.srv.serve_forever, daemon=True).start()
        cls.opener = urllib.request.build_opener(_NoFollow)

    @classmethod
    def tearDownClass(cls):
        cls.srv.shutdown()

    def setUp(self):
        app._hits.clear()
        counts.reset_latest()
        con = sqlite3.connect(app.DB_PATH)
        con.executescript("DELETE FROM checks; DELETE FROM updates;")
        con.commit()
        con.close()

    def get(self, path):
        try:
            r = self.opener.open(self.base + path, timeout=5)
            return r.status, r.headers.get("Location")
        except urllib.error.HTTPError as e:
            return e.code, e.headers.get("Location")

    def q(self, sql):
        con = sqlite3.connect(app.DB_PATH)
        try:
            return con.execute(sql).fetchall()
        finally:
            con.close()

    def test_check_counts_valid_and_silently_drops_bad(self):
        ok = "/v1/check?v=0.9.4&p=linux-x86_64&via=cli&n=1&w=1&m=1"
        self.assertEqual(self.get(ok)[0], 204)
        self.assertEqual(self.get(ok)[0], 204)
        self.assertEqual(self.get("/v1/check?v=0.9.4-dev.abc1234&p=darwin-arm64&via=mcp&n=0&w=0&m=0")[0], 204)
        bad = [
            "/v1/check?v=0.9.4&p=linux-x86_64&via=cli&n=1&w=1",  # missing flag
            "/v1/check?v=0.9.4&p=solaris&via=cli&n=0&w=0&m=0",
            "/v1/check?v=../x&p=linux-x86_64&via=cli&n=0&w=0&m=0",
            "/v1/check?v=0.9.4&p=linux-x86_64&via=web&n=0&w=0&m=0",
            "/v1/check?v=0.9.4&p=linux-x86_64&via=cli&n=2&w=0&m=0",
            "/v1/check",
        ]
        for path in bad:
            self.assertEqual(self.get(path)[0], 204, path)  # the client must never retry
        rows = self.q("SELECT version, platform, via, n, w, m, count FROM checks ORDER BY version")
        self.assertEqual(rows, [("0.9.4", "linux-x86_64", "cli", 1, 1, 1, 2),
                                ("0.9.4-dev.abc1234", "darwin-arm64", "mcp", 0, 0, 0, 1)])

    def test_nothing_identifying_is_stored(self):
        self.get("/v1/check?v=0.9.4&p=linux-x86_64&via=cli&n=0&w=0&m=0&id=secret-machine-id")
        dump = "\n".join(str(r) for t in ("checks", "updates") for r in self.q("SELECT * FROM " + t))
        self.assertNotIn("secret", dump)
        self.assertNotIn("127.0.0.1", dump)

    def test_latest_redirects_like_github(self):
        counts.latest_tag(fetch=lambda: "v0.9.4")
        status, loc = self.get("/v1/latest?v=0.9.3&p=darwin-arm64")
        self.assertEqual(status, 302)
        self.assertEqual(loc, "https://github.com/GeRiY/callgraph/releases/tag/v0.9.4")
        self.get("/v1/latest?v=garbage&p=???")  # still redirected, counted as unknown
        rows = self.q("SELECT event, version, platform, target, count FROM updates ORDER BY version")
        self.assertEqual(rows, [("check", "0.9.3", "darwin-arm64", "v0.9.4", 1), ("check", "?", "?", "v0.9.4", 1)])

    def test_latest_unknown_gives_503(self):
        self.assertIsNone(counts.latest_tag(fetch=lambda: None))
        old = counts.fetch_latest_tag
        counts.fetch_latest_tag = lambda opener=None: None
        try:
            self.assertEqual(self.get("/v1/latest?v=0.9.3&p=linux-x86_64")[0], 503)
        finally:
            counts.fetch_latest_tag = old

    def test_a_stale_tag_is_refreshed_before_answering(self):
        counts.latest_tag(fetch=lambda: "v0.9.4")
        stale = counts._latest["at"] + counts.LATEST_TTL + 1
        self.assertEqual(counts.latest_tag(fetch=lambda: "v0.9.5", now=stale), "v0.9.5")
        # GitHub does not answer: the stale tag is still served (until LATEST_MAX_AGE)
        stale = counts._latest["at"] + counts.LATEST_TTL + 1
        self.assertEqual(counts.latest_tag(fetch=lambda: None, now=stale), "v0.9.5")

    def test_latest_cache_is_not_served_when_too_old(self):
        counts.latest_tag(fetch=lambda: "v0.9.4")
        later = counts._latest["at"] + counts.LATEST_MAX_AGE + 1
        self.assertIsNone(counts.latest_tag(fetch=lambda: None, now=later))

    def test_download_redirects_only_release_files(self):
        status, loc = self.get("/v1/download/v0.9.4/callgraph-linux-x86_64.zip?k=update&v=0.9.3")
        self.assertEqual((status, loc), (302, "https://github.com/GeRiY/callgraph/releases/download/v0.9.4/callgraph-linux-x86_64.zip"))
        status, loc = self.get("/v1/download/latest/callgraph-darwin-arm64.zip?k=install")
        self.assertEqual((status, loc), (302, "https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-darwin-arm64.zip"))
        self.assertEqual(self.get("/v1/download/v0.9.4/SHA256SUMS")[0], 302)
        self.assertEqual(self.get("/v1/download/v0.9.4/callgraph.mcpb?k=install")[0], 302)
        for path in [
            "/v1/download/v0.9.4/evil.sh",
            "/v1/download/v0.9.4/../../x",
            "/v1/download/main/callgraph-linux-x86_64.zip",
            "/v1/download/v0.9.4/callgraph-linux-x86_64.zip/extra",
            "/v1/download/v0.9.4/https:%2F%2Fevil.example",
            "/v1/download//callgraph-linux-x86_64.zip",
        ]:
            status, loc = self.get(path)
            self.assertEqual(status, 404, path)
            self.assertIsNone(loc, path)
        rows = self.q("SELECT event, version, platform, target, count FROM updates ORDER BY event, platform")
        self.assertEqual(rows, [("install", "?", "?", "v0.9.4", 1), ("install", "?", "darwin-arm64", "latest", 1),
                                ("update", "0.9.3", "linux-x86_64", "v0.9.4", 1)])

    def test_over_budget_still_redirects_but_does_not_count(self):
        old = app.COUNT_LIMIT
        app.COUNT_LIMIT = 2
        try:
            codes = [self.get("/v1/download/v0.9.4/callgraph-linux-x86_64.zip?k=update")[0] for _ in range(5)]
        finally:
            app.COUNT_LIMIT = old
        self.assertEqual(codes, [302] * 5)
        self.assertEqual(self.q("SELECT SUM(count) FROM updates")[0][0], 2)


class SchemaTwo(unittest.TestCase):
    """schema 2 (v0.9.5): command-order edges and session bands next to the counters."""

    @classmethod
    def setUpClass(cls):
        app.db().close()
        cls.srv = ThreadingHTTPServer(("127.0.0.1", 0), app.Handler)
        cls.url = "http://127.0.0.1:%d/v1" % cls.srv.server_address[1]
        threading.Thread(target=cls.srv.serve_forever, daemon=True).start()

    @classmethod
    def tearDownClass(cls):
        cls.srv.shutdown()

    def setUp(self):
        app._hits.clear()

    def post(self, body):
        req = urllib.request.Request(self.url, data=json.dumps(body).encode(), headers={"Content-Type": "application/json"})
        try:
            return urllib.request.urlopen(req, timeout=5).status
        except urllib.error.HTTPError as e:
            return e.code

    def v2(self, batch, edges=None, sessions=None):
        body = payload(batch=batch)
        body["schema"] = 2
        body["days"][0]["edges"] = edges if edges is not None else {
            "start>locate|none|-": 2, "locate>outline|same_project|<5s": 3,
            "search!>search|same_project|5-60s": 1, "outline>end|none|-": 2, "other": 4}
        body["days"][0]["sessions"] = sessions if sessions is not None else {
            "count": 2, "len": [0, 2, 0, 0, 0], "dur": [1, 1, 0, 0], "rep_max": [2, 0, 0]}
        return body

    def q(self, sql):
        con = sqlite3.connect(app.DB_PATH)
        try:
            return con.execute(sql).fetchall()
        finally:
            con.close()

    def test_edges_and_sessions_are_stored_once_per_batch(self):
        con = sqlite3.connect(app.DB_PATH); con.executescript("DELETE FROM edges; DELETE FROM session_stats;"); con.commit(); con.close()
        self.assertEqual(self.post(self.v2("b2b2-1")), 204)
        self.assertEqual(self.post(self.v2("b2b2-1")), 204)  # same batch again: nothing doubles
        rows = dict(((src, dst, err, rel, gap), n) for src, dst, err, rel, gap, n in
                    self.q("SELECT src, dst, err, rel, gap, count FROM edges"))
        self.assertEqual(rows[("locate", "outline", 0, "same_project", "<5s")], 3)
        self.assertEqual(rows[("search", "search", 1, "same_project", "5-60s")], 1)
        self.assertEqual(rows[("start", "locate", 0, "none", "-")], 2)
        self.assertEqual(rows[("other", "other", 0, "-", "-")], 4)
        self.assertEqual(self.q("SELECT count, len FROM session_stats"), [(2, "[0, 2, 0, 0, 0]")])
        self.assertEqual(self.post(self.v2("b2b2-2")), 204)  # another batch of the same day adds up
        self.assertEqual(self.q("SELECT count FROM edges WHERE src='locate' AND dst='outline'"), [(6,)])

    def test_rejects_names_paths_and_bad_shapes(self):
        bad_edges = [
            {"locate>/home/user/app.php|same_file|<5s": 1},
            {"locate>outline|same_file|forever": 1},
            {"locate>outline|my_project|<5s": 1},
            {"Locate>outline|none|-": 1},
            {"locate>outline|none|-": -1},
            {"locate>outline|none|-": True},
            {f"locate>c{i}|none|-": 1 for i in range(app.MAX_EDGES + 1)},
        ]
        for edges in bad_edges:
            self.assertEqual(self.post(self.v2("cccc-1", edges=edges)), 422, str(edges)[:80])
        for sessions in [{"count": 1, "len": [1, 2]}, {"count": -1}, {"count": 1, "names": [1]}]:
            self.assertEqual(self.post(self.v2("cccc-2", sessions=sessions)), 422, sessions)
        v1 = payload(batch="cccc-3")
        v1["days"][0]["edges"] = {"start>locate|none|-": 1}
        self.assertEqual(self.post(v1), 422)  # schema 1 cannot carry edges

    def test_uses_are_stored_and_validated(self):
        con = sqlite3.connect(app.DB_PATH); con.execute("DELETE FROM uses"); con.commit(); con.close()
        body = self.v2("eeee-1")
        body["days"][0]["uses"] = {"locate>outline|f:hit1|s:-": 5, "outline>method_body|f:hit|s:hit": 2,
                                   "search!>show|f:miss|s:miss": 1, "other": 3}
        self.assertEqual(self.post(body), 204)
        self.assertEqual(self.post(body), 204)  # same batch: no doubling
        rows = dict(((a, b, e, f, s), n) for a, b, e, f, s, n in
                    self.q("SELECT src, dst, err, f, s, count FROM uses"))
        self.assertEqual(rows[("locate", "outline", 0, "hit1", "-")], 5)
        self.assertEqual(rows[("search", "show", 1, "miss", "miss")], 1)
        self.assertEqual(rows[("other", "other", 0, "-", "-")], 3)
        for bad in [{"locate>outline|f:top|s:-": 1}, {"locate>outline|f:hit1": 1},
                    {"locate>/etc/passwd|f:hit1|s:-": 1}, {"start>locate|f:-|s:-": 1},
                    {"locate>outline|f:hit1|s:hit1": 1}]:
            b = self.v2("eeee-2"); b["days"][0]["uses"] = bad
            self.assertEqual(self.post(b), 422, bad)

    def test_schema_one_still_accepted(self):
        self.assertEqual(self.post(payload(batch="dddd-0001")), 204)


if __name__ == "__main__":
    unittest.main(verbosity=1)
