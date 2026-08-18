# ADR-0002: Git history is the audit trail

- **Status:** Accepted
- **Date:** 2026-08-18

## Context

A board is asked questions about the past: when did this start, who moved it,
how long was it in `doing`, who wrote it. Most tools answer them by keeping
their own log — timestamps in the record, an events table, an activity feed.

The board already lives in a git repository, and every change to it is a
change to a tracked file.

## Decision

**The repository's history is the only history.** agile-md keeps no log of its
own: no `moved-at` keys, no events file, no activity feed, no `reporter`
field — who created a ticket is already in `git log --follow`.

Concretely:

- Moving a ticket uses `git mv` when the file is tracked, so the rename is
  recorded and `--follow` reconstructs the ticket's life. Untracked files fall
  back to `fs::rename`.
- **`amd` never commits.** It stages the rename; the user decides when it is
  recorded and what it is recorded alongside.
- Archiving a ticket (`amd rm`, aliased `amd junk`) moves it to the
  gitignored `archive/`. Because
  `git mv` refuses an ignored destination — and forcing it would put the
  archived ticket back into the history that `.gitignore` exists to keep out — a tracked
  ticket is `git rm --cached`'d and then moved.

## Consequences

- `git log --follow tasks/done/007-*.md` is the ticket's biography, with no
  feature to maintain.
- Ticket changes review like any other diff, in the same PR as the code.
- Duplicating history into frontmatter is always the wrong answer, however
  convenient the field would be.
- **Cost:** history only exists once the user commits. An uncommitted board
  has no trail. Accepted — the alternative is a tool that commits on your
  behalf, which is worse.
- **Ids are never reused.** An archived ticket's id stays spent, because
  reusing it would silently repoint every `parent` and `related` that named
  it. `<board>/.next-id` is the authority (it remembers ids whose tickets are
  gone, which no scan can); the scan of the columns is the safety net against
  a lost or badly merged counter.

## Alternatives considered

- **Timestamps in the frontmatter on every move.** Rejected: a second history
  that can contradict the first, and a merge conflict on every concurrent
  move.
- **Committing automatically.** Rejected: it takes the user's commit history
  away from them, and a tool that writes to your branch without being asked is
  a tool you stop trusting.

## See also

"Never commits" is a rule about local tools acting on someone's working tree.
A hosted component operating on its own clone would have to commit; that
scoping is recorded in [ADR-0008](0008-room-for-a-server.md), not decided
here.
