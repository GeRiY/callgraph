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

    def test_rate_limit(self):
        app._hits.clear()
        codes = [self.post(payload(batch="dddd-%d" % i)) for i in range(app.RATE_LIMIT + 2)]
        self.assertIn(429, codes)
        app._hits.clear()


if __name__ == "__main__":
    unittest.main(verbosity=1)
