#!/usr/bin/env python3
"""Check that status pages serve valid Atlassian Statuspage JSON.

    python3 scripts/verify_services.py                 # every entry in services.json
    python3 scripts/verify_services.py https://x.example.com ...   # specific base URLs
    python3 scripts/verify_services.py --candidates    # the initial candidate list

Exit code is non-zero if any URL fails. Standard library only.
"""
import json
import sys
import urllib.request
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


def check(base: str) -> str:
    url = base.rstrip("/") + "/api/v2/summary.json"
    req = urllib.request.Request(url, headers={"User-Agent": "status-tray-verify"})
    with urllib.request.urlopen(req, timeout=15) as resp:
        data = json.load(resp)
    indicator = data["status"]["indicator"]
    assert isinstance(data.get("components", []), list)
    assert isinstance(data.get("incidents", []), list)
    return f"ok (indicator={indicator}, components={len(data.get('components', []))})"


def main() -> int:
    args = sys.argv[1:]
    if args == ["--candidates"]:
        urls = CANDIDATES
    elif args:
        urls = args
    else:
        catalog = json.loads((Path(__file__).resolve().parent.parent / "services.json").read_text())
        urls = [s["url"] for s in catalog["services"]]
    failed = 0
    for u in urls:
        try:
            print(f"{u}: {check(u)}")
        except Exception as e:  # noqa: BLE001 - report anything
            failed += 1
            print(f"{u}: FAIL ({type(e).__name__}: {e})")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
