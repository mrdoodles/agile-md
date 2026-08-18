# Architecture decision records

Why agile-md is built the way it is. Each record captures one decision, the
context that forced it, and what it costs — so that a later change is made
deliberately rather than by drift.

These live next to the code on purpose. An ADR and the commit that violates it
can then move together, and a reviewer sees both in one diff.

| # | Decision | Status |
|---|----------|--------|
| [0001](0001-status-is-the-folder.md) | Status is the folder | Accepted |
| [0002](0002-git-history-is-the-audit-trail.md) | Git history is the audit trail | Accepted |
| [0003](0003-tickets-are-rendered-from-templates.md) | Tickets are rendered from templates | Accepted |
| [0004](0004-one-library-two-interfaces.md) | One library, two interfaces | Accepted |
| [0005](0005-settings-state-and-precedence.md) | Settings, state and precedence | Accepted (precedence open) |
| [0006](0006-the-operation-vocabulary.md) | The operation vocabulary | Proposed |
| [0007](0007-concurrency-and-locking.md) | Concurrency and locking | Proposed |
| [0008](0008-room-for-a-server.md) | Room for a server | Accepted |

## Statuses

- **Proposed** — written down to be argued with. Not yet binding.
- **Accepted** — binding. Code that contradicts it is a bug in the code or an
  ADR that needs superseding.
- **Superseded by NNNN** — kept, never deleted. The reasoning that led
  somewhere wrong is worth as much as the reasoning that led somewhere right.

## Writing one

Copy the shape of an existing record: **Context** (the forces, not the
solution), **Decision** (what we do, in the present tense), **Consequences**
(including what it costs us), **Alternatives considered** (and why they lost).

Numbers are never reused, and a record is never edited to say something
different — it is superseded by a new one.
