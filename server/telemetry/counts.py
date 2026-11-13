"""Anonymous usage and update counting for callgraph (check.callgraph.eu, update.callgraph.eu).

- GET /v1/check?v=&p=&via=&n=&w=&m=   once per install and UTC day, on the first real use → daily counter, 204.
- GET /v1/latest?v=&p=                the update check → counter, 302 to the GitHub tag page of the latest
                                       release (the same Location the GitHub releases/latest redirect gives).
- GET /v1/download/<tag>/<asset>?k=   update / install download → counter, 302 to the GitHub release file.

No identifier exists anywhere: the client only sends its version, platform and yes/no flags it computed
from its own local state. Client addresses are only used in memory for the request budget, never stored.
Only daily aggregate counters are written.
"""

import re
import threading
import time
import urllib.error
import urllib.request
from datetime import date
from urllib.parse import parse_qs

REPO = "https://github.com/GeRiY/callgraph"
PLATFORMS = ("darwin-arm64", "darwin-x86_64", "linux-x86_64", "linux-aarch64", "windows-x86_64")
VERSION_RE = re.compile(r"^\d{1,3}\.\d{1,3}\.\d{1,3}(-dev\.[0-9a-f]{7})?$")
TAG_RE = re.compile(r"^v\d{1,3}\.\d{1,3}\.\d{1,3}$")
# only the files a release really has can be redirected to (no open redirect)
ASSET_RE = re.compile(
    r"^(callgraph-(darwin-arm64|darwin-x86_64|linux-x86_64|linux-aarch64|windows-x86_64)\.zip"
    r"|callgraph-\d{1,3}\.\d{1,3}\.\d{1,3}\.tar\.gz|SHA256SUMS|callgraph\.mcpb)$"
)
VIA = ("cli", "mcp")
KINDS = ("update", "install")
LATEST_TTL = 300  # seconds the cached latest tag is fresh
LATEST_MAX_AGE = 86400  # an older cached tag is not served (503 → the client falls back to GitHub)

SCHEMA = """
CREATE TABLE IF NOT EXISTS checks (
    day TEXT NOT NULL, version TEXT NOT NULL, platform TEXT NOT NULL, via TEXT NOT NULL,
    n INTEGER NOT NULL, w INTEGER NOT NULL, m INTEGER NOT NULL, count INTEGER NOT NULL,
    PRIMARY KEY (day, version, platform, via, n, w, m)
);
CREATE TABLE IF NOT EXISTS updates (
    day TEXT NOT NULL, event TEXT NOT NULL, version TEXT NOT NULL, platform TEXT NOT NULL,
    target TEXT NOT NULL, count INTEGER NOT NULL,
    PRIMARY KEY (day, event, version, platform, target)
);
"""

_latest_lock = threading.Lock()
_latest = {"tag": None, "at": 0.0, "refreshing": False}


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def fetch_latest_tag(opener=None) -> str | None:
    """The tag GitHub's releases/latest redirects to (read from the Location, without following it)."""
    opener = opener or urllib.request.build_opener(_NoRedirect)
    try:
        opener.open(urllib.request.Request(REPO + "/releases/latest", method="HEAD"), timeout=5)
    except urllib.error.HTTPError as e:
        location = e.headers.get("Location", "") if 300 <= e.code < 400 else ""
        tag = location.rsplit("/releases/tag/", 1)[-1] if "/releases/tag/" in location else ""
        return tag if TAG_RE.match(tag) else None
    except (urllib.error.URLError, OSError, ValueError):
        return None
    return None


def reset_latest() -> None:
    with _latest_lock:
        _latest.update(tag=None, at=0.0, refreshing=False)


def _refresh_latest(fetch) -> None:
    tag = fetch()
    with _latest_lock:
        if tag:
            _latest["tag"], _latest["at"] = tag, time.time()
        _latest["refreshing"] = False


def latest_tag(fetch=None, now: float | None = None) -> str | None:
    """The cached latest tag. A stale one is refreshed BEFORE answering (right after a release the old tag would
    otherwise send the client to GitHub directly, uncounted); if GitHub does not answer, the stale tag is served —
    never one older than LATEST_MAX_AGE. Requests arriving during a refresh get the cached tag."""
    fetch = fetch or fetch_latest_tag
    now = time.time() if now is None else now
    with _latest_lock:
        tag, at = _latest["tag"], _latest["at"]
        start = (tag is None or now - at > LATEST_TTL) and not _latest["refreshing"]
        if start:
            _latest["refreshing"] = True
    if start:
        _refresh_latest(fetch)
        with _latest_lock:
            tag, at = _latest["tag"], _latest["at"]
        now = time.time() if tag and at > now else now
    if tag is None or now - at > LATEST_MAX_AGE:
        return None
    return tag


def _one(query: dict, key: str) -> str:
    return (query.get(key) or [""])[0]


def parse_check(query_string: str) -> tuple | None:
    """(version, platform, via, n, w, m) of a valid check; None for anything else."""
    q = parse_qs(query_string, keep_blank_values=True)
    version, platform, via = _one(q, "v"), _one(q, "p"), _one(q, "via")
    flags = [{"0": 0, "1": 1}.get(_one(q, k)) for k in ("n", "w", "m")]
    if not VERSION_RE.match(version) or platform not in PLATFORMS or via not in VIA or None in flags:
        return None
    return (version, platform, via, *flags)


def parse_client(query_string: str) -> tuple[str, str]:
    """Version and platform of an update request; unknown values count as '?' (the redirect still works)."""
    q = parse_qs(query_string, keep_blank_values=True)
    version, platform = _one(q, "v"), _one(q, "p")
    return (version if VERSION_RE.match(version) else "?", platform if platform in PLATFORMS else "?")


def parse_download(path: str, query_string: str) -> tuple[str, str, str] | None:
    """(tag, asset, kind) of /v1/download/<tag>/<asset>; None for anything that is not a release file."""
    parts = path.split("/")
    if len(parts) != 5 or parts[:3] != ["", "v1", "download"]:
        return None
    tag, asset = parts[3], parts[4]
    if not (tag == "latest" or TAG_RE.match(tag)) or not ASSET_RE.match(asset):
        return None
    kind = _one(parse_qs(query_string, keep_blank_values=True), "k")
    return tag, asset, kind if kind in KINDS else "other"


def download_url(tag: str, asset: str) -> str:
    if tag == "latest":
        return f"{REPO}/releases/latest/download/{asset}"
    return f"{REPO}/releases/download/{tag}/{asset}"


def asset_platform(asset: str) -> str:
    for p in PLATFORMS:
        if f"-{p}." in asset:
            return p
    return "?"


def counted_asset(asset: str) -> bool:
    """Only the packages count as a download (the checksum file is fetched alongside every update)."""
    return asset.endswith(".zip") or asset.endswith(".mcpb") or asset.endswith(".tar.gz")


def count_check(con, row: tuple) -> None:
    con.execute(
        "INSERT INTO checks(day, version, platform, via, n, w, m, count) VALUES (?,?,?,?,?,?,?,1) "
        "ON CONFLICT(day, version, platform, via, n, w, m) DO UPDATE SET count=count+1",
        (date.today().isoformat(), *row),
    )


def count_update(con, event: str, version: str, platform: str, target: str) -> None:
    con.execute(
        "INSERT INTO updates(day, event, version, platform, target, count) VALUES (?,?,?,?,?,1) "
        "ON CONFLICT(day, event, version, platform, target) DO UPDATE SET count=count+1",
        (date.today().isoformat(), event, version, platform, target),
    )
