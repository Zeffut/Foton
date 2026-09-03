# Player-report issue sync

Foton stores player reports in `dev/bug-reports.jsonl` so the public reports
page can be built without a database. Each new report also creates one GitHub
issue. The report card links to that issue once it has been committed.

GitHub is the status authority:

- an open or reopened issue keeps the report **open**;
- close it with the `fixed` label to mark the report **fixed** on the site;
- close it with the `not-a-bug` label to mark it **not a defect** on the site.

A closed issue without exactly one of those labels is intentionally not
published as resolved: the website must not guess why it was closed. Applying
one of the labels sends another webhook and completes the synchronization.

## Deployment setup

Set these Vercel environment variables for the production deployment:

| Variable | Purpose |
|---|---|
| `FOTON_REPORT_TOKEN` | Bearer token used by Foton servers when posting `/bug` reports. |
| `FOTON_GITHUB_TOKEN` | GitHub fine-grained token for `Zeffut/Foton`, with **Issues: read/write** and **Contents: read/write**. |
| `FOTON_GITHUB_WEBHOOK_SECRET` | A new random secret shared only with the GitHub webhook. |
| `FOTON_REPORT_REPO` | Optional repository override; defaults to `Zeffut/Foton`. |

Then create a GitHub repository webhook:

- Payload URL: `https://foton.zeffut.fr/api/github-issues/`
- Content type: `application/json`
- Secret: the exact value of `FOTON_GITHUB_WEBHOOK_SECRET`
- Event: **Issues** only
- SSL verification: enabled

The endpoint verifies GitHub's `X-Hub-Signature-256` before it reads an issue.
It ignores issues without the `foton-report` label, so ordinary project issues
cannot affect a player report. On the first report, Foton creates the labels
`foton-report`, `category:<name>`, `fixed`, and `not-a-bug` if they are absent.

The function commits the report before it creates the issue. If GitHub is
temporarily unavailable, the report is retained in history and retrying the
same game request resumes it instead of adding another report.
