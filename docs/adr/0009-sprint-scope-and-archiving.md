# ADR-0009: Sprint scope, and the one thing a started sprint refuses

- **Status:** Accepted
- **Date:** 2026-08-18

## Context

A started sprint used to be immutable: `Board::set_epic` refused to move a
ticket in ("nothing more can be added") or out ("its tickets are fixed"). The
intent was that a sprint's scope is a commitment, so fixing it makes a forecast
line well defined.

Two things were wrong with that.

**It did not hold.** `amd rm` archived a ticket straight out of a started
sprint with no complaint at all. The rule was enforced at one door and not the
other, so scope could shrink silently through the one nobody guarded.

**It described a world that does not exist.** Teams move tickets in and out of
running sprints — more often when they are new to the practice. It is poor
practice and it does skew the charts. But a tool that refuses does not stop it
happening; it stops it happening *through the tool*. People edit the
frontmatter by hand or drag the file in Finder, which skews the charts **and**
loses the record of what changed.

## Decision

**A started sprint takes tickets in and lets them out.** The sizing rule stays:
a sprint still refuses an unsized ticket, because an unsized ticket makes the
sprint's total a lie, and that total is the one number a sprint is for.

**A started sprint refuses to have a ticket archived out of it.** Take it out
of the sprint first, then archive it — two steps, and the record survives the
first.

The asymmetry is not about how drastic the two actions feel. It is about what
each leaves behind:

| | What it does | What history keeps |
|---|---|---|
| Move out of a sprint | `git mv` to another folder | The rename, dated. The ticket is still on the board with its points readable. |
| Archive from a sprint | `git rm --cached` + move into gitignored `archive/` | A deletion. The ticket is off the board and its points are only recoverable by digging in history. |

A burnup can draw the first. It cannot honestly draw the second — the points
would simply stop existing, with nothing on the board explaining when or why.
Requiring the two-step means every scope change is a dated, visible event
before anything disappears.

## Consequences

- **`State::Started` marks a moment, not a lock.** It is the origin of the
  chart's x-axis and the baseline scope for the ideal line — not a mutation
  gate. `Group::accepts_changes()` is now consulted by `Board::archive` alone.
- **Burnup earns its place.** Under a genuinely fixed scope, a burnup is a flat
  line and adds nothing a burndown does not already show. With scope free to
  move, the gap between the scope line and the completed line is the thing
  worth looking at.
- **A sprint's scope is now a query over the whole board, not a directory
  listing.** `Board::tasks_in_group` scans every column, because a ticket in
  `doing/` still belongs to the sprint it was committed to. Counting only
  `backlog/` made a sprint's points *fall* as work progressed, which is the
  opposite of what the number means.
- Inside `backlog/` the folder decides which group a ticket is in; outside it
  there are no group directories, so the `epic` frontmatter key is the only
  carrier. The two are not interchangeable and `tasks_in_group` treats them
  separately.
- **Cost:** the tool no longer defends the commitment. That defence was
  illusory — it was one `amd rm` wide — and the charts are a better teacher
  than a refusal.

## Alternatives considered

- **Keep the sprint immutable and close the `amd rm` hole.** Rejected: it makes
  the tool refuse something teams legitimately need to do, and the workaround
  (hand-editing) is worse than the act.
- **Allow archiving with `--force`.** Rejected as the default answer: it turns
  a question about the record into a question about how sure you are. The
  two-step is no harder and leaves history intact.
- **Record `removed-from-sprint` events in the ticket.** Rejected: a second
  history that can contradict the first ([ADR-0002](0002-git-history-is-the-audit-trail.md)).
