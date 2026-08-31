"""Receives a bug report from a running Foton server and commits it.

The repository is the database. A report arrives here, is appended to
`dev/bug-reports.jsonl` through the GitHub contents API, and the push that
creates rebuilds the site -- so the page is generated from a committed file
exactly like every other fact the site states. No database, no second service,
and a report that reaches the page is a report anyone can find in the history.

Standard library only: the site's build installs nothing, and this should not
be the one thing that needs a package.
"""

import base64
import json
import os
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler

API = "https://api.github.com"
DATA_PATH = "dev/bug-reports.jsonl"
BRANCH = "master"

# Longest body accepted, in bytes. A report is a few hundred bytes; anything
# near this is not one.
MAX_BODY = 64 * 1024

# The contents API needs the file's current SHA, so two reports landing
# together make the second one stale. Reading and writing again is the whole
# fix at this volume.
WRITE_ATTEMPTS = 3

# Fields kept from what the server sends. The account identifier is
# deliberately absent: it does not help reproduce anything, and this file is
# public.
KEEP = ("at", "player", "world", "position", "category", "description", "version")


def _github(method, path, token, payload=None):
    """One GitHub API call. Returns (status, parsed body)."""
    request = urllib.request.Request(
        f"{API}{path}",
        method=method,
        data=json.dumps(payload).encode() if payload is not None else None,
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "Content-Type": "application/json",
            "User-Agent": "foton-bug-reports",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=15) as response:
            return response.status, json.loads(response.read() or b"{}")
    except urllib.error.HTTPError as error:
        return error.code, {}


def _append(report, token, repo):
    """Appends one record to the data file, retrying a stale write."""
    for _ in range(WRITE_ATTEMPTS):
        status, current = _github(
            "GET", f"/repos/{repo}/contents/{DATA_PATH}?ref={BRANCH}", token
        )
        if status == 200:
            existing = base64.b64decode(current.get("content", "")).decode("utf-8")
            sha = current.get("sha")
        elif status == 404:
            existing, sha = "", None
        else:
            return status

        number = sum(1 for line in existing.splitlines() if line.strip()) + 1
        record = {"number": number}
        record.update({key: report[key] for key in KEEP if key in report})
        record["status"] = "open"
        updated = existing + ("" if existing.endswith("\n") or not existing else "\n")
        updated += json.dumps(record, ensure_ascii=False) + "\n"

        payload = {
            "message": f"report: #{number} [{report.get('category', 'other')}] "
            f"from {report.get('player', 'a player')}",
            "content": base64.b64encode(updated.encode("utf-8")).decode("ascii"),
            "branch": BRANCH,
        }
        if sha:
            payload["sha"] = sha
        status, _ = _github("PUT", f"/repos/{repo}/contents/{DATA_PATH}", token, payload)
        if status in (200, 201):
            return 202
        # 409 means someone else committed between the read and the write.
        if status != 409:
            return status
    return 409


class handler(BaseHTTPRequestHandler):
    """Vercel's Python runtime entry point."""

    def _reply(self, status, message):
        body = json.dumps({"status": message}).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):  # noqa: N802 - the runtime dictates the name
        expected = os.environ.get("FOTON_REPORT_TOKEN")
        github_token = os.environ.get("FOTON_GITHUB_TOKEN")
        repo = os.environ.get("FOTON_REPORT_REPO", "Zeffut/Foton")
        if not expected or not github_token:
            # Refusing loudly beats accepting reports into nowhere.
            self._reply(503, "report intake is not configured")
            return

        if self.headers.get("Authorization") != f"Bearer {expected}":
            self._reply(401, "unauthorized")
            return

        try:
            length = int(self.headers.get("Content-Length") or 0)
        except ValueError:
            self._reply(400, "bad length")
            return
        if length <= 0 or length > MAX_BODY:
            self._reply(413, "body out of range")
            return

        try:
            report = json.loads(self.rfile.read(length))
        except (ValueError, UnicodeDecodeError):
            self._reply(400, "body is not JSON")
            return
        if not isinstance(report, dict) or not str(report.get("description", "")).strip():
            self._reply(422, "a report needs a description")
            return

        result = _append(report, github_token, repo)
        if result == 202:
            self._reply(202, "filed")
        else:
            # The server keeps its own copy, so a failure here costs freshness
            # rather than the report. Saying so plainly is what lets a 5xx be
            # retried and a 4xx not be.
            self._reply(502 if result >= 500 or result == 409 else 400, "could not commit")

    def do_GET(self):  # noqa: N802 - the runtime dictates the name
        self._reply(405, "post a report here")
