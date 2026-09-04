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

## Fixing a report

`bash dev/reports.sh` lists what is still open, asking GitHub rather than
trusting the committed statuses, which lag a webhook behind.
`bash dev/reports.sh 14` prints one report in full: who filed it, on which
version, and the world and position they were standing at.

A report is a symptom, written in French by a player from memory. It is not a
diagnosis, and two reports are often one defect -- #16 and #17 both describe
the smithing table. Read them together, fix the cause once, close each with its
own note.

### The loop, per report

1. **Locate the defect.** Find the code that produces the symptom. If the
   report cannot be tied to code, stop and say so on the issue: a guess costs
   more than an open report.
2. **Read vanilla first.** Open the matching class under
   `minecraft-src/minecraft/src/net/minecraft/` and transpose it. Rule 1 of
   `AGENTS.md` is not relaxed for bug fixes -- most player reports *are*
   parity gaps.
3. **Fix on a branch** named `fix/report-<n>-<slug>`, in conventional commits.
   `master` takes no direct commits and prek enforces that.
4. **Add a test only if it would catch the regression again.** A test that
   restates the fix is noise. Reports #1 and #3 earned one.
5. **`bash dev/ci.sh` green before merging.** Never `--no-verify`: those hooks
   are the only thing between a fix and a red `master`.

### Closing a report

Two writes, in this order. Reversed, they race.

1. **Write the note, commit, push.** `note` on the report's line in
   `dev/bug-reports.jsonl` is what the player reads on the reports page: one
   English sentence saying what was actually done -- *"The kinetic_weapon
   component was never executed. Implemented."* No code writes this field. It
   is the only part of the record a human owns.
2. **Then close the issue, with a label:**

```bash
gh issue edit <n> --add-label fixed
gh issue close <n> --reason completed
```

Closing fires the webhook, which reads `dev/bug-reports.jsonl` back from GitHub
and rewrites `status` in it. A note pushed first survives that; a note pushed
afterwards has to be merged against the bot.

Three rules the endpoint already enforces, and that a closing has to respect:

- **Never set `status` by hand.** GitHub owns it. A local edit is overwritten
  by the next webhook, or fights it.
- **The label is not optional.** A closed issue carrying neither `fixed` nor
  `not-a-bug` is deliberately not published as resolved -- the site will not
  guess why something was closed -- so the player still sees their report as
  open. `dev/reports.sh` prints these as `AMBIGUOUS`.
- **Never both labels.** Together they cancel out, with the same result.

### When it is not a defect

`--add-label not-a-bug`, and close with `--reason "not planned"`. The note is
still owed: *"A test of the form, filed as one. Not a defect."* answers the
player completely.

### When it cannot be fixed

Leave the issue open and comment what was checked -- the files read, the
vanilla class compared against, why it did not reproduce. An open report with a
trail is worth more than a closed one without.

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
