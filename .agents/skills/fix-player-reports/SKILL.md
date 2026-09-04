---
name: fix-player-reports
description: >-
  Use this skill when the user asks to work on the Foton player bug reports --
  the ones filed in game with the /bug command, listed by `bash dev/reports.sh`
  and tracked as GitHub issues labelled `foton-report`. It covers finding the
  defect behind a report, fixing it against the vanilla source, and closing the
  report so the reporting site actually shows it resolved.
---

# Fixing Foton player reports

Players file reports in game with `/bug`. Each one lands in
`dev/bug-reports.jsonl` and opens a GitHub issue. The reports page on the site
shows the player what became of theirs.

## Read before anything else

**`REPORTING.md`, the section "Fixing a report".** It holds the rules that
cannot be guessed from the code: which half of the system owns `status`, why
the note is pushed *before* the issue is closed, and why closing without a
label leaves the player still seeing their report as open. Follow it exactly.

Then `AGENTS.md` for the engineering standard, and `CLAUDE.md` for how to build
and verify on this machine.

## Steps

1. `bash dev/reports.sh` — what is still open, asked of GitHub rather than of
   the possibly stale committed statuses. `bash dev/reports.sh <n>` prints one
   report in full.
2. **Locate the defect.** Find the code producing the symptom. A report is a
   symptom written in French from memory, not a diagnosis.
3. **Read vanilla first**, under `minecraft-src/minecraft/src/net/minecraft/`.
   Most player reports are parity gaps; rule 1 of `AGENTS.md` is not relaxed
   for bug fixes.
4. **Group reports that share a cause**, and fix the cause once.
5. **Fix on a branch** `fix/report-<n>-<slug>`, in conventional commits.
   `master` takes no direct commits.
6. **Gate on `bash dev/ci.sh`.** Never `--no-verify`.
7. **Close as `REPORTING.md` prescribes**: note committed and pushed first,
   then the label, then the close.

## Where to stop

Do not close a report you did not actually fix. One that will not reproduce
earns a comment on its issue saying what was checked, and stays open. A guess
costs more than an open report.
