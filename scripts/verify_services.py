#!/usr/bin/env python3
"""Check that catalog entries serve data their provider can parse.

    python3 scripts/verify_services.py                 # every entry in services.json (by type)
    python3 scripts/verify_services.py https://x.example.com ...   # statuspage base URLs
    python3 scripts/verify_services.py --candidates    # the initial statuspage candidates

Types: statuspage, google-cloud (incidents.json), aws-health (UTF-16 JSON), rss-active (RSS/Atom).

Exit code is non-zero if any URL fails. Standard library only.
"""
import json
import sys
import urllib.request
import xml.etree.ElementTree as ET
from pathlib import Path

CANDIDATES = [
    "https://www.githubstatus.com",
    "https://www.cloudflarestatus.com",
    "https://status.openai.com",
    "https://status.claude.com",
    "https://status.anthropic.com",
    "https://bitbucket.status.atlassian.com",
    "https://discordstatus.com",
    "https://status.npmjs.org",
    "https://www.vercel-status.com",
    "https://status.digitalocean.com",
    "https://linearstatus.com",
    "https://status.sentry.io",
    "https://status.circleci.com",
]


def fetch(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "status-tray-verify"})
    with urllib.request.urlopen(req, timeout=15) as resp:
        return resp.read()


def decode_text(raw: bytes) -> str:
    """UTF-16 (BOM or BOM-less) with UTF-8 fallback; mirrors the Rust decoder."""
    if raw[:2] == b"\xff\xfe":
        return raw[2:].decode("utf-16-le", "replace")
    if raw[:2] == b"\xfe\xff":
        return raw[2:].decode("utf-16-be", "replace")
    if raw[:3] == b"\xef\xbb\xbf":
        return raw[3:].decode("utf-8", "replace")
    if len(raw) >= 2 and raw[0] != 0 and raw[1] == 0:
        return raw.decode("utf-16-le", "replace")
    if len(raw) >= 2 and raw[0] == 0 and raw[1] != 0:
        return raw.decode("utf-16-be", "replace")
    return raw.decode("utf-8", "replace")


def check_statuspage(base: str) -> str:
    data = json.loads(fetch(base.rstrip("/") + "/api/v2/summary.json"))
    indicator = data["status"]["indicator"]
    assert isinstance(data.get("components", []), list)
    assert isinstance(data.get("incidents", []), list)
    return f"ok (indicator={indicator}, components={len(data.get('components', []))})"


def check_google_cloud(base: str) -> str:
    data = json.loads(fetch(base.rstrip("/") + "/incidents.json"))
    assert isinstance(data, list), "incidents.json is not a list"
    open_ = [i for i in data if not i.get("end")]
    for i in data[:5]:
        assert "id" in i and "status_impact" in i, "unexpected incident shape"
    return f"ok (incidents={len(data)}, open={len(open_)})"


def check_aws_health(base: str) -> str:
    data = json.loads(decode_text(fetch(base.rstrip("/") + "/public/currentevents")).lstrip("\ufeff"))
    assert isinstance(data, list), "currentevents is not a list"
    for e in data:
        assert "arn" in e and "status" in e, "unexpected event shape"
    open_ = [e for e in data if str(e["status"]) != "0"]
    return f"ok (events={len(data)}, open={len(open_)})"


def check_rss_active(feed_url: str) -> str:
    root = ET.fromstring(fetch(feed_url))
    tag = root.tag.rsplit("}", 1)[-1]
    assert tag in ("rss", "feed"), f"unexpected XML root <{tag}>"
    items = [e for e in root.iter() if e.tag.rsplit("}", 1)[-1] in ("item", "entry")]
    return f"ok ({tag}, active items={len(items)})"


CHECKS = {
    "statuspage": check_statuspage,
    "google-cloud": check_google_cloud,
    "aws-health": check_aws_health,
    "rss-active": check_rss_active,
}


def main() -> int:
    args = sys.argv[1:]
    if args and args[0] in ("-h", "--help"):
        print(__doc__)
        return 0
    if args == ["--candidates"]:
        targets = [("statuspage", u) for u in CANDIDATES]
    elif args:
        targets = [("statuspage", u) for u in args]
    else:
        catalog = json.loads((Path(__file__).resolve().parent.parent / "services.json").read_text())
        targets = [(s["type"], s["url"]) for s in catalog["services"]]
    failed = 0
    for typ, u in targets:
        try:
            if typ not in CHECKS:
                raise ValueError(f"unknown type {typ!r}")
            print(f"{u}: {CHECKS[typ](u)}")
        except Exception as e:  # noqa: BLE001 - report anything
            failed += 1
            print(f"{u}: FAIL ({type(e).__name__}: {e})")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
