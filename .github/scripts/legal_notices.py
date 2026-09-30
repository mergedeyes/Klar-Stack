#!/usr/bin/env python3
"""Notices about changed Terms / privacy policy, from klar-web/legal-updates.

Each notice is a Markdown file written in the pull request that changes a
legal page:

    documents: terms, privacy
    ---
    A short summary in plain German of what changed.

`check BASE HEAD` (CI, on pull requests) fails when the Terms or the privacy
page changed without a new or edited notice naming that document, unless the
PR has the label "legal: no notice" (typo fixes). It also validates every
notice file.

`publish` (frontend deploy) waits until the live legal pages show this
deploy's "Stand" date, then sends every notice file to the backend, which
publishes each file once (the file name is the key) and ignores the rest.
"""

import json
import os
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request

NOTICE_DIR = "klar-web/legal-updates"
# The document each page is, and where it is live.
PAGES = {
    "terms": ("klar-web/src/app/nutzungsbedingungen/page.tsx", "/nutzungsbedingungen"),
    "privacy": ("klar-web/src/app/datenschutz/page.tsx", "/datenschutz"),
}
NO_NOTICE_LABEL = "legal: no notice"
KEY = re.compile(r"^[a-z0-9-]+\.md$")
SUMMARY_MIN, SUMMARY_MAX = 20, 2000


def parse(path):
    """Returns (documents, summary) or raises ValueError with the problem."""
    name = os.path.basename(path)
    if not KEY.match(name) or len(name) > 100:
        raise ValueError(f"{name}: file names are lowercase letters, digits and dashes, ending in .md")
    with open(path, encoding="utf-8") as f:
        text = f.read()
    head, sep, summary = text.partition("\n---\n")
    if not sep:
        raise ValueError(f"{name}: needs a 'documents: ...' line, then a line with ---, then the summary")
    match = re.fullmatch(r"documents:\s*(.+)", head.strip())
    if not match:
        raise ValueError(f"{name}: the first line must be 'documents: terms', 'documents: privacy' or both")
    documents = sorted({d.strip() for d in match.group(1).split(",") if d.strip()})
    if not documents or any(d not in PAGES for d in documents):
        raise ValueError(f"{name}: documents must be terms and/or privacy, got {match.group(1).strip()!r}")
    summary = summary.strip()
    if not SUMMARY_MIN <= len(summary) <= SUMMARY_MAX:
        raise ValueError(f"{name}: the summary must be {SUMMARY_MIN} to {SUMMARY_MAX} characters")
    return documents, summary


def notice_files():
    if not os.path.isdir(NOTICE_DIR):
        return []
    return sorted(os.path.join(NOTICE_DIR, n) for n in os.listdir(NOTICE_DIR) if n.endswith(".md"))


def git(*args):
    return subprocess.run(["git", *args], check=True, capture_output=True, text=True).stdout


def check(base, head):
    errors = []
    parsed = {}
    for path in notice_files():
        try:
            parsed[path] = parse(path)
        except ValueError as e:
            errors.append(str(e))

    changed = set(git("diff", "--name-only", f"{base}...{head}").split())
    labels = json.loads(os.environ.get("PR_LABELS") or "[]")
    covered = set()
    for path in changed:
        if path.startswith(NOTICE_DIR + "/") and path in parsed:
            covered.update(parsed[path][0])
    for document, (page, _) in PAGES.items():
        if page in changed and document not in covered:
            if NO_NOTICE_LABEL in labels:
                print(f"{page} changed without a notice; allowed by the label {NO_NOTICE_LABEL!r}")
            else:
                errors.append(
                    f"{page} changed, but no notice in {NOTICE_DIR}/ names '{document}'. "
                    f"Add a file there (see .github/scripts/legal_notices.py), or add the PR label "
                    f"{NO_NOTICE_LABEL!r} for a change that needs no notice (e.g. a typo)."
                )

    for e in errors:
        print(f"::error::{e}")
    if errors:
        sys.exit(1)
    print(f"Legal notices OK ({len(parsed)} file(s))")


def stand(page):
    """This deploy's Stand date for a page, as the frontend workflow stamps it."""
    return git("log", "-1", "--first-parent", "--date=format-local:%d.%m.%Y", "--format=%cd", "--", page).strip()


def wait_until_live(site):
    """Waits (up to 10 minutes) until every legal page shows its new Stand."""
    for document, (page, url_path) in PAGES.items():
        date = stand(page)
        if not date:
            continue
        url = site.rstrip("/") + url_path
        for _ in range(60):
            try:
                with urllib.request.urlopen(url, timeout=20) as res:
                    if f"Stand: {date}" in res.read().decode("utf-8", "replace"):
                        print(f"{url} shows Stand {date}")
                        break
            except urllib.error.URLError as e:
                print(f"{url}: {e}")
            time.sleep(10)
        else:
            print(f"::error::{url} still doesn't show Stand {date}; not publishing notices yet")
            sys.exit(1)


# The deploy workflow waits for the backend deploy of the same commit
# first; this is the safety net on top (a backend that's restarting, a slow
# rollout). 404 (endpoint not there yet), 5xx and connection errors are
# retried for up to 30 minutes; anything else (e.g. 401, a wrong token)
# fails at once.
RETRY_STATUSES = {404, 502, 503, 504}
RETRY_ATTEMPTS, RETRY_PAUSE = 90, 20


def send(req):
    for attempt in range(1, RETRY_ATTEMPTS + 1):
        try:
            with urllib.request.urlopen(req, timeout=30) as res:
                return json.loads(res.read())
        except urllib.error.HTTPError as e:
            if e.code not in RETRY_STATUSES or attempt == RETRY_ATTEMPTS:
                raise
            print(f"{req.full_url}: HTTP {e.code}, the backend may still be deploying; retrying in {RETRY_PAUSE} s")
        except urllib.error.URLError as e:
            if attempt == RETRY_ATTEMPTS:
                raise
            print(f"{req.full_url}: {e.reason}; retrying in {RETRY_PAUSE} s")
        time.sleep(RETRY_PAUSE)


def publish():
    api, token, site = os.environ.get("API_URL"), os.environ.get("LEGAL_UPDATES_TOKEN"), os.environ.get("SITE_URL")
    files = notice_files()
    if not files:
        return
    if not api or not token:
        print("::warning::API_URL or LEGAL_UPDATES_TOKEN isn't set: legal notices were not published")
        return
    if site:
        wait_until_live(site)
    else:
        print("::warning::SITE_URL isn't set: publishing without waiting for the new pages")

    for path in files:
        documents, summary = parse(path)
        body = json.dumps({"key": os.path.basename(path), "documents": documents, "summary": summary}).encode()
        req = urllib.request.Request(
            api.rstrip("/") + "/internal/legal-updates",
            data=body,
            method="POST",
            headers={"Content-Type": "application/json", "Authorization": f"Bearer {token}"},
        )
        created = send(req).get("created")
        print(f"{os.path.basename(path)}: {'published' if created else 'already published'}")


if __name__ == "__main__":
    if sys.argv[1:2] == ["check"] and len(sys.argv) == 4:
        check(sys.argv[2], sys.argv[3])
    elif sys.argv[1:] == ["publish"]:
        publish()
    else:
        sys.exit("usage: legal_notices.py check BASE HEAD | publish")
