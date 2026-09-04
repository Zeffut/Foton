---
description: Work the player reports filed in game with /bug — locate each against vanilla, fix it, and close it on GitHub with its label and its note.
argument-hint: "[a report number, or a category, or nothing for every open report]"
allowed-tools: Bash, Read, Edit, Write, Glob, Grep, Task, Agent, TodoWrite
---

Work the player reports filed with the in-game `/bug` command.

Target: $ARGUMENTS
(a number → that report; a category → that category; empty → every open one)

## Read before anything else

**`REPORTING.md`, the section "Fixing a report".** It holds the rules that
cannot be guessed from the code: which half of the system owns `status`, why
the note is pushed *before* the issue is closed, and why closing without a
label leaves the player still seeing their report as open. Follow it exactly —
nothing below replaces it.

Then `AGENTS.md` for the engineering standard, and `CLAUDE.md` for how to build
and verify on this machine.

## What is open right now

!`bash dev/reports.sh`

Use `bash dev/reports.sh <n>` for the full record of one: who filed it, on
which version, and the world and position they were standing at.

## How to work

1. **Triage in parallel.** Dispatch one read-only agent per report, each
   answering one question: *where does this symptom come from, and what does
   the vanilla class do instead?* Each returns the file and line it lands on,
   the vanilla reference under `minecraft-src/`, and how sure it is. No agent
   edits anything in this phase.

2. **Group before fixing.** Triage will show reports that share one cause —
   #16 and #17 both describe the smithing table. Fix the cause once, close each
   report with its own note.

3. **Fix sequentially**, one branch per cause, `fix/report-<n>-<slug>`. The
   working tree and the cargo lock are shared; parallel fixes fight over both.

4. **Gate every fix on `bash dev/ci.sh`** before merging. Never `--no-verify`.

5. **Close each report** exactly as `REPORTING.md` prescribes: note committed
   and pushed first, then `gh issue edit --add-label`, then `gh issue close`.

## Where to stop

Do not close a report you did not actually fix. One that will not reproduce, or
that cannot be tied to code, earns a comment on its issue saying what was
checked — the files read, the vanilla class compared against — and stays open.
A guess costs more than an open report.

Finish by stating, per report: fixed, not a defect, or still open — one line
each, and for the ones left open, why.
