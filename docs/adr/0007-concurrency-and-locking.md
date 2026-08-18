# ADR-0007: Concurrency and locking

- **Status:** Proposed
- **Date:** 2026-08-18

## Context

A board used to have one writer at a time. It now has several: a desktop board
left open, a command line in a terminal, and an agent working the CLI — all
against the same files, at the same time, with no coordination.

The board reloads after its own mutations and offers a Reload button, but
nothing watches the filesystem. So: an agent edits ticket 7 via the CLI; the
open board still holds the copy it read minutes ago; the user saves the
editor; the agent's change is gone, silently, with no error anywhere.

Two problems get called "locking", and they need separating:

1. **Lost update** — a writer overwrites a change it never saw. A correctness
   bug. Happens whether or not anyone wanted exclusivity.
2. **Coordination** — "I am working on this, leave it alone." A workflow
   feature, deliberately invoked by a person or an agent.

## Decision

### 1. Optimistic writes, always on, invisible

Every read that may lead to a write records what it read — the file's content
hash. Every write checks the file still matches before replacing it, and fails
if it does not.

- No user-facing state, no ceremony, nothing to remember to release.
- Fixes the lost-update bug even when nobody has locked anything.
- The failure is a typed error the interface can act on: the board can offer
  "reload and reapply", the CLI can tell an agent to re-read and retry.
- Depends on [ADR-0006](0006-the-operation-vocabulary.md)'s one-write-per-
  action: four separate writes means four separate races.

### 2. Advisory leases, explicit, in the ticket

Coordination is a **lease**, not a mutex, and the wording matters because of
what git cannot do (below).

- Held in the ticket's frontmatter — `locked-by`, `locked-at` — not a sidecar
  file. It travels with the board, shows up in review, and needs no second
  store. Consistent with [ADR-0002](0002-git-history-is-the-audit-trail.md).
- The holder is `git config user.email`, which is already how the tool knows
  who you are.
- **A lease expires.** The honest answer to "who releases a lock held by
  someone on holiday" is "it lapses." A lapsed lease is not an error state
  and needs no administrator.
- Taking a lease is an operation; releasing it is an operation; **breaking
  someone else's** is a third, explicit and separate — never a side effect of
  a normal write.
- A held lease causes writes by anyone else to fail with a distinct typed
  error, so an agent gets a clean, specific refusal rather than a surprise.

### What this cannot do

In a git-backed board there is no mutex. A lease is a line in a file, and
another machine learns of it only after a pull. It stops the *local* tools
from writing and it tells a human or an agent that someone has claimed the
ticket. It does not prevent a concurrent edit on another clone — that
collision surfaces as a merge conflict, which is the correct place for it.

This is why it is called a lease. Naming it a lock would promise a guarantee
the design cannot keep.

## Consequences

- The lost-update bug is fixed for everyone without anyone opting in.
- `locked-by` in frontmatter means taking a lease dirties the working tree and
  appears in diffs. Accepted: a claim nobody can see is not a claim.
- Two more things a ticket template may want to render, and two more keys in
  the `TaskContext` public API ([ADR-0003](0003-tickets-are-rendered-from-templates.md)).
- **Cost:** an expired lease that someone still believed in is a surprise. The
  expiry has to be visible on the card and in `amd show`, not just enforced.

## Open questions

- Lease duration, and whether it is configurable per board.
- Does taking a lease block *moving* a ticket, or only editing its content?
- Does `amd start` take a lease automatically? It is the one command that
  already means "I am working on this."
- Does the board show other people's leases as a badge, or only refuse on
  save?

## Alternatives considered

- **Locking only, no optimistic writes.** Rejected: it leaves the actual bug
  unfixed for everyone who does not lock, which will be almost everyone.
- **A lock file in `.locks/`, gitignored.** Rejected: per-machine, so it
  cannot coordinate the people it is meant to coordinate.
- **Filesystem locks (`flock`).** Rejected: they do not survive a process
  exiting, do not cross machines, and cannot express "Sam has this ticket
  today."
