"""File player reports as GitHub issues and mirror their state to the site.

The committed JSONL file remains the site's offline source of truth. GitHub is
the work queue: an incoming report creates one issue, and the signed ``issues``
webhook writes its status back into that file. Standard library only.
"""

import base64
import hashlib
import hmac
import json
import os
import re
import urllib.error
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler

API = "https://api.github.com"
DATA_PATH = "dev/bug-reports.jsonl"
BRANCH = "master"
MAX_REPORT_BODY = 64 * 1024
MAX_WEBHOOK_BODY = 256 * 1024
WRITE_ATTEMPTS = 3
KEEP = ("at", "player", "world", "position", "category", "description", "version")
REPORT_LABEL = "foton-report"
FIXED_LABEL = "fixed"
NOT_A_BUG_LABEL = "not-a-bug"


def _github(method, path, token, payload=None):
    """One GitHub API call. Returns ``(status, parsed_json)``."""
    request = urllib.request.Request(
        f"{API}{path}", method=method,
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
    except (OSError, ValueError):
        return 502, {}


def _read_reports(token, repo):
    status, current = _github("GET", f"/repos/{repo}/contents/{DATA_PATH}?ref={BRANCH}", token)
    if status == 200:
        try:
            text = base64.b64decode(current.get("content", "")).decode("utf-8")
            records = [json.loads(line) for line in text.splitlines() if line.strip()]
            return 200, records, current.get("sha")
        except (ValueError, UnicodeDecodeError):
            return 502, [], None
    if status == 404:
        return 200, [], None
    return status, [], None


def _write_reports(records, sha, token, repo, message):
    text = "".join(json.dumps(record, ensure_ascii=False) + "\n" for record in records)
    payload = {
        "message": message,
        "content": base64.b64encode(text.encode("utf-8")).decode("ascii"),
        "branch": BRANCH,
    }
    if sha:
        payload["sha"] = sha
    return _github("PUT", f"/repos/{repo}/contents/{DATA_PATH}", token, payload)[0]


def _report_key(report):
    """A stable key makes a client retry resume the same report.

    The game assigns ``number`` before forwarding. It distinguishes two reports
    with identical words from a retry of one local report.
    """
    kept = {key: report.get(key) for key in KEEP if key in report}
    kept["source_number"] = report.get("number")
    return hashlib.sha256(json.dumps(kept, sort_keys=True).encode()).hexdigest()


def _category_label(category):
    normalized = re.sub(r"[^a-z0-9_-]+", "-", str(category).lower()).strip("-")
    return f"category:{normalized or 'other'}"


def _record_report(report, token, repo):
    """Commit the report once, retrying only an optimistic-lock conflict."""
    key = _report_key(report)
    for _ in range(WRITE_ATTEMPTS):
        status, records, sha = _read_reports(token, repo)
        if status != 200:
            return status, None
        for record in records:
            if record.get("report_key") == key:
                return 202, record

        record = {"number": len(records) + 1, "report_key": key, "status": "open"}
        record.update({field: report[field] for field in KEEP if field in report})
        result = _write_reports(
            records + [record], sha, token, repo,
            f"report: #{record['number']} [{record.get('category', 'other')}]",
        )
        if result in (200, 201):
            return 202, record
        if result != 409:
            return result, None
    return 409, None


def _ensure_label(label, color, token, repo):
    """Create a label if needed; GitHub returns 422 when it already exists."""
    status, _ = _github("POST", f"/repos/{repo}/labels", token, {"name": label, "color": color})
    return status in (201, 422)


def _issue_marker(record):
    return f"<!-- foton-report:{record['report_key']} -->"


def _quote(text):
    return "\n".join(f"> {line}" if line else ">" for line in str(text).splitlines())


def _issue_body(record):
    return (
        f"{_issue_marker(record)}\n## Player report\n\n"
        f"{_quote(record.get('description', ''))}\n\n## Context\n\n"
        f"- Player: `{record.get('player', '')}`\n"
        f"- Category: `{record.get('category', '')}`\n"
        f"- World: `{record.get('world', '')}`\n"
        f"- Position: `{record.get('position', '')}`\n"
        f"- Foton: `{record.get('version', '')}`\n"
    )


def _find_existing_issue(record, token, repo):
    query = urllib.parse.urlencode({"state": "all", "labels": REPORT_LABEL, "per_page": 100})
    status, issues = _github("GET", f"/repos/{repo}/issues?{query}", token)
    if status != 200 or not isinstance(issues, list):
        return status, None
    marker = _issue_marker(record)
    for issue in issues:
        if marker in str(issue.get("body") or ""):
            return 200, issue
    return 404, None


def _open_issue(record, token, repo):
    labels = (
        (REPORT_LABEL, "b0741a"),
        (_category_label(record.get("category", "other")), "6f6858"),
        (FIXED_LABEL, "0e8a16"),
        (NOT_A_BUG_LABEL, "c8c0af"),
    )
    if not all(_ensure_label(label, color, token, repo) for label, color in labels):
        return 502, None
    status, existing = _find_existing_issue(record, token, repo)
    if status == 200:
        return 201, existing
    if status != 404:
        return status, None
    title = f"[{record.get('category', 'other')}] {str(record.get('description', '')).splitlines()[0][:90]}"
    status, issue = _github("POST", f"/repos/{repo}/issues", token, {
        "title": title, "body": _issue_body(record),
        "labels": [REPORT_LABEL, _category_label(record.get("category", "other"))],
    })
    return status, issue if status == 201 else None


def _attach_issue(record, issue, token, repo):
    """Save GitHub's durable issue ID on the report after creation."""
    for _ in range(WRITE_ATTEMPTS):
        status, records, sha = _read_reports(token, repo)
        if status != 200:
            return status, None
        for current in records:
            if current.get("report_key") != record["report_key"]:
                continue
            if current.get("issue_number"):
                return 202, current
            current["issue_number"] = issue.get("number")
            current["issue_url"] = issue.get("html_url")
            result = _write_reports(
                records, sha, token, repo,
                f"report: link #{current['number']} to issue #{current['issue_number']}",
            )
            if result in (200, 201):
                return 202, current
            if result != 409:
                return result, None
            break
        else:
            return 404, None
    return 409, None


def _refresh_issue_status(record, token, repo):
    """Close the small gap where GitHub closes an issue before the link commit."""
    status, issue = _github("GET", f"/repos/{repo}/issues/{record['issue_number']}", token)
    if status != 200:
        return status
    return _sync_status(issue, token, repo)


def _file(report, token, repo):
    status, record = _record_report(report, token, repo)
    if status != 202:
        return status
    if not record.get("issue_number"):
        status, issue = _open_issue(record, token, repo)
        if status != 201:
            return status
        status, record = _attach_issue(record, issue, token, repo)
        if status != 202:
            return status
    return _refresh_issue_status(record, token, repo)


def _label_names(issue):
    return {label.get("name") if isinstance(label, dict) else label for label in issue.get("labels", [])}


def _issue_status(issue):
    """Return a site status only when GitHub gives an unambiguous one."""
    if issue.get("state") == "open":
        return "open"
    labels = _label_names(issue)
    if FIXED_LABEL in labels and NOT_A_BUG_LABEL not in labels:
        return "fixed"
    if NOT_A_BUG_LABEL in labels and FIXED_LABEL not in labels:
        return "closed"
    return None


def _sync_status(issue, token, repo):
    status = _issue_status(issue)
    if status is None:
        return 202
    issue_number = issue.get("number")
    for _ in range(WRITE_ATTEMPTS):
        result, records, sha = _read_reports(token, repo)
        if result != 200:
            return result
        for record in records:
            if record.get("issue_number") == issue_number:
                if record.get("status") == status:
                    return 202
                record["status"] = status
                written = _write_reports(records, sha, token, repo, f"report: sync issue #{issue_number} as {status}")
                if written in (200, 201):
                    return 202
                if written != 409:
                    return written
                break
        else:
            return 202
    return 409


def _valid_signature(secret, body, signature):
    expected = "sha256=" + hmac.new(secret.encode(), body, hashlib.sha256).hexdigest()
    return bool(signature) and hmac.compare_digest(expected, signature)


class handler(BaseHTTPRequestHandler):
    """Vercel's Python runtime entry point for intake and GitHub webhooks."""

    def _reply(self, status, message):
        body = json.dumps({"status": message}).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _read_body(self, limit):
        try:
            length = int(self.headers.get("Content-Length") or 0)
        except ValueError:
            return None
        if length <= 0 or length > limit:
            return None
        return self.rfile.read(length)

    def _github_webhook(self, event, body, token, repo):
        secret = os.environ.get("FOTON_GITHUB_WEBHOOK_SECRET")
        if not secret:
            self._reply(503, "GitHub webhook is not configured")
            return
        if not _valid_signature(secret, body, self.headers.get("X-Hub-Signature-256")):
            self._reply(401, "invalid GitHub signature")
            return
        if event != "issues":
            self._reply(202, "ignored")
            return
        try:
            issue = json.loads(body)["issue"]
        except (KeyError, ValueError, TypeError):
            self._reply(400, "invalid GitHub payload")
            return
        if REPORT_LABEL not in _label_names(issue):
            self._reply(202, "ignored")
            return
        result = _sync_status(issue, token, repo)
        self._reply(202 if result == 202 else 502, "synchronized" if result == 202 else "could not synchronize")

    def do_POST(self):  # noqa: N802 - the runtime dictates the name
        github_token = os.environ.get("FOTON_GITHUB_TOKEN")
        repo = os.environ.get("FOTON_REPORT_REPO", "Zeffut/Foton")
        if not github_token:
            self._reply(503, "report intake is not configured")
            return
        event = self.headers.get("X-GitHub-Event")
        body = self._read_body(MAX_WEBHOOK_BODY if event else MAX_REPORT_BODY)
        if body is None:
            self._reply(413, "body out of range")
            return
        if event:
            self._github_webhook(event, body, github_token, repo)
            return

        expected = os.environ.get("FOTON_REPORT_TOKEN")
        if not expected or self.headers.get("Authorization") != f"Bearer {expected}":
            self._reply(401, "unauthorized")
            return
        try:
            report = json.loads(body)
        except (ValueError, UnicodeDecodeError):
            self._reply(400, "body is not JSON")
            return
        if not isinstance(report, dict) or not str(report.get("description", "")).strip():
            self._reply(422, "a report needs a description")
            return
        result = _file(report, github_token, repo)
        self._reply(202 if result == 202 else 502, "filed" if result == 202 else "could not file")

    def do_GET(self):  # noqa: N802 - the runtime dictates the name
        self._reply(405, "post a report here")
