# ADR-0008: Room for a server

- **Status:** Accepted
- **Date:** 2026-08-18

## Context

There may one day be a hosted component — a server, a GitHub App — supporting
team working, with an agent component inside the application. It is a
possibility, not a commitment, and nothing about it is specified.

That leaves two ways to get it wrong, and they are both expensive:

- **Build for it now.** Storage traits, async, an auth model, a sync protocol
  — abstractions invented against an imaginary requirement, which are almost
  always the wrong ones, and which have to be carried and maintained while
  they earn nothing.
- **Ignore it entirely.** A handful of decisions being taken *this month* bake
  a single local user into the library's signatures. Undoing those later is
  the "complete rewrite" this record exists to avoid.

## Decision

**We do not build a server, or design for one.** We take three constraints
now — each of which is cheap today, invasive later, and defensible on its own
merits regardless of whether a server is ever built — and we explicitly defer
everything else.

### 1. The library never discovers its own context

`Board::at(path)` is the API. `Board::locate()` — current directory plus
`git rev-parse --show-toplevel` — is a convenience for front ends that have a
current directory to speak of.

Both already exist ([board.rs:86](../../src/board.rs#L86) and
[board.rs:102](../../src/board.rs#L102)). The constraint is that **operations
take a board and never call `locate()` internally**, so no library code path
depends on there being one current repository.

Defensible anyway: it is what makes the desktop board's multi-repository view
possible, and what lets tests construct a board without changing directory.

### 2. Identity is a parameter, not an ambient fact

Today the library reads `git config user.name` and `user.email` from inside
`Draft::prepare` ([create.rs:145](../../src/create.rs#L145)). That is correct
for one person at one terminal and wrong for anything else: a server acts for
many users, and an agent acts *on behalf of* someone who is not the process
owner.

An actor is passed into operations. Front ends resolve it however suits them —
`git config` for the CLI and the board, a session for anything hosted.

This is the expensive one to retrofit: it changes the signature of every
operation that records who did something, and
[ADR-0007](0007-concurrency-and-locking.md)'s lease holder depends on it.

Defensible anyway: it makes `@me` resolution testable and stops the library
shelling out to `git` to answer a question the caller already knows.

### 3. No process-global state in `amd-lib`

The two `OnceLock`s in the codebase are in `form.rs` and `render.rs`, both
bound for `amd-cli` under [ADR-0004](0004-one-library-two-interfaces.md). The
library must not acquire any of its own: a hosted component is concurrent and
serves more than one caller, and process-global configuration is how that
becomes a bug nobody can reproduce.

Defensible anyway: process-global state makes parallel tests unreliable.

## Deliberately deferred

Not to be built, designed or accommodated until there is a real requirement:

- A storage abstraction. The board is a directory of markdown
  ([ADR-0001](0001-status-is-the-folder.md)); a trait over "somewhere a board
  might live" would be invented against a specification that does not exist.
- **Async.** A server can run blocking filesystem work on a thread pool.
  Making the library async now infects every caller and every test to buy
  something a `spawn_blocking` provides for free.
- Networking, authentication, authorisation, multi-tenancy, a sync protocol,
  webhooks, and any dependency that exists to serve them.

## The tension to name now

[ADR-0002](0002-git-history-is-the-audit-trail.md) says **`amd` never
commits** — the user decides when a change is recorded. That is a decision
about *local tools acting on someone's working tree*, where committing on
their behalf would be a trespass.

A hosted component operating on its own clone has no such working tree and
must commit and push to do anything at all. When that day comes, ADR-0002 is
**scoped, not overturned**: the rule keeps applying to `amd` and `amdui`.
Recording it here so it arrives as a known boundary rather than as a
contradiction someone discovers mid-implementation.

## Consequences

- Three constraints, all small, none requiring new machinery.
- A server, if it ever exists, is a fourth crate (`amd-server`) on the same
  library — additive to the workspace, changing nothing that is already there.
- [ADR-0006](0006-the-operation-vocabulary.md) turns out to be most of the
  groundwork: typed operations, typed failures and JSON output are what an
  HTTP layer would sit on. [ADR-0007](0007-concurrency-and-locking.md) is the
  multi-writer story, and compare-and-write does not care whether the second
  writer is a colleague or a process.
- The "two" in ADR-0004's title is a count of today's interfaces, not a limit.
- **Cost:** three constraints taken partly on speculation. Each is justified
  by testability alone, so the speculative part costs nothing if the server
  never happens.
