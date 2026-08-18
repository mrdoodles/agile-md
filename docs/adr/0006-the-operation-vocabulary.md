# ADR-0006: The operation vocabulary

- **Status:** Proposed
- **Date:** 2026-08-18

## Context

[ADR-0004](0004-one-library-two-interfaces.md) puts one library under two
interfaces. That prevents terminal code leaking into the board, but on its own
it does not stop the interfaces drifting apart in *capability* — which is the
drift that has actually happened. The board grew ordering and inline editing
while the command line did not; `amd ls` still sorts by id and ignores the
`order` key the board writes, so the two front ends disagree about what a
column looks like.

"Both interfaces should be consistent" is an intention. It needs a mechanism.

There is also a second consumer now. Agents drive the CLI, and an agent needs
different things from a human: parseable output, distinguishable failures, and
no prompt it cannot answer.

## Decision

### One vocabulary, two translators

`amd-lib` exposes an enumerable set of **operations**. Each is a typed request
with typed success and typed failure. The interfaces do nothing but translate:
`clap` arguments into an operation, or a click into an operation.

Neither interface may know a rule the other does not. "A sprint refuses an
unsized ticket" is enforced once, in the library, and both front ends get the
same refusal for the same reason.

### The set

| Operation | CLI | GUI |
|---|---|---|
| Create board | `amd init` | prompt on first run |
| Create ticket | `amd new` | new-ticket overlay |
| Read ticket | `amd show` | card / editor |
| List board | `amd board`, `amd ls` | the columns |
| Move column | `amd start`/`done`/`back` | drag across |
| Reorder within column | `amd set <ref> order` | drag up/down |
| Set title / points / epic | `amd set <ref> <key>` | editor fields |
| Set body | `amd edit` | rich-text editor |
| Assign | `amd assign` | editor field |
| Junk | `amd rm` | — *(gap)* |
| Link tickets | `amd link` | — *(gap)* |
| Create epic / sprint | `amd group epic`/`sprint` | Add epic / Add sprint |
| Start sprint | `amd group start` | backlog view |
| Start work (branch) | `amd start` | — *(deliberate: branch switching is a terminal act)* |
| List / add / remove repos | `amd repos` | checkboxes |
| Templates | `amd templates` | — *(gap)* |
| View all repositories at once | — *(gap)* | the default |

Gaps are listed rather than hidden. Each is either closed or recorded as a
deliberate asymmetry with a reason.

### Parity is a test, not a discipline

Because the operations are an enumerable set, a test can assert that every one
has a CLI route. ADRs record decisions; tests are what stop the decision being
forgotten. Deliberate asymmetries live in an explicit allow-list with a
comment, so skipping one is a visible act.

### One write per user action

Today, saving the GUI's editor performs four independent read-modify-write
cycles (assignee, title, points, body), each re-reading the whole file. A
failure on the third leaves a half-saved ticket, and there is no single place
to enforce an invariant across fields.

An operation writes **once**. That is what makes validation possible in one
place, and it is a precondition for
[ADR-0007](0007-concurrency-and-locking.md).

### Machine-readable output and typed failure

- Every listing/read operation offers `--json`. The rich tree is a display
  format and is explicitly allowed to change; agents parse the JSON.
- Failures are typed in the library and mapped to **distinct exit codes** by
  the CLI. "No such ticket", "not a git repository" and "the sprint refused an
  unsized ticket" must be distinguishable without matching on error strings.

## Consequences

- A new capability is added to the library once, and both interfaces can
  reach it. Adding it to only one becomes a visible choice.
- The ordering disagreement is a single comparator in the library —
  `(priority, order, id)` — and both front ends inherit it.
- **Cost:** an operation layer is indirection. It is justified by there being
  two callers with a hard consistency requirement, and it should stay thin —
  if an operation is only ever a pass-through to one library function, it
  should *be* that function.
- **Cost:** `--json` is a second output contract to keep stable, and once an
  agent depends on it, it is as public as the CLI itself.

## Alternatives considered

- **A shared command enum both front ends dispatch through.** Rejected as
  the primary mechanism: it pushes GUI concerns (which button, which modal
  state) into a shared type. The library should expose capabilities; how a
  front end reaches them is its own business.
- **Documenting the API and reviewing for parity.** Rejected: that is what we
  have now.
