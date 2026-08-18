# ADR-0005: Settings, state and precedence

- **Status:** Accepted. The precedence section is **Open** — see below.
- **Date:** 2026-08-18

## Context

Two kinds of thing are persisted outside the board today: the desktop board's
theme and text size, and the list of registered repositories. More will
follow. They are currently two files under `~/.config/agile-md/` — `config`
(flat `key = value`, unknown keys preserved) and `repos` (one path per line).

[ADR-0004](0004-one-library-two-interfaces.md) says only `amd-lib` touches the
filesystem, which raises the question of how a GUI-only setting gets written
by a crate that must not know what a theme is.

## Decision

### Three concerns, not two

| | What it is | Written | Read by |
|---|---|---|---|
| **App settings** | User preferences, interface-agnostic | When a human changes one | CLI + GUI |
| **UI settings** | Theme, text size, window geometry | When a human changes one | GUI |
| **Registry** | Accumulated state — which repositories have been seen | Automatically, on almost every command | CLI + GUI |

App settings and UI settings share one file, namespaced (`ui.*` for the GUI's
keys). **The registry stays a separate file.** It is not a preference: it is
written as a side effect by `registry::remember()` whenever a command resolves
a board. Putting it in the settings file would mean every `amd board` rewrites
the file holding the user's preferences.

The registry is *app-scoped* — both interfaces read it — but scope and storage
are different questions, and the answer differs for each.

### The current repository is derived, never stored

`git rev-parse --show-toplevel` answers "which repository am I in", and
`Registry::boards()` folds it in whether or not it is on the list. Only the
*list* is persisted. Storing "the current repo" would create a second source
of truth for something git already answers — see
[ADR-0001](0001-status-is-the-folder.md) for the general form of that mistake.

### The library owns the file; the front end owns the schema

- **`amd-lib`** owns path resolution, the file format, atomic writes,
  preservation of keys it does not recognise, and the rule that a missing or
  broken settings file is an empty one rather than an error — a bad config
  must never stop you seeing the board.
- **`amd-ui`** owns `Theme`, `FontSize` and whatever follows. It hands the
  library typed key/values within its namespace and asks for them back.

`amd-lib` never learns what `nord` means, so no UI vocabulary enters it, and
[ADR-0004](0004-one-library-two-interfaces.md)'s rule holds without an
exception for settings.

### Load, modify, save — as one operation

Settings are never loaded once and held for a session. A GUI that holds a copy
from startup and writes it on exit will silently discard anything the CLI
wrote in between. Every write re-reads first. This is the same class of bug as
[ADR-0007](0007-concurrency-and-locking.md) addresses for tickets, and it gets
the same answer.

### App-tier content

The app tier is empty today. What belongs in it is defaults for what are
currently environment-variable-only knobs: board directory (`AMD_DIR`), branch
types (`AMD_BRANCH_TYPES`), auto-registration (`AMD_NO_REGISTER`), the editor,
and which column `amd new` lands in. All of these are needed by the CLI — the
app tier is the *shared* tier, and only `ui.*` is GUI-only.

## Open: precedence

**Not yet decided.** Recorded here so it is decided deliberately rather than
by whichever line of code runs last.

The recommendation to argue with:

> **flag > environment variable > settings file > built-in default.**

The reasoning: agents drive the CLI, so a flag must be able to force
deterministic behaviour regardless of what a human has configured in a window.
If the file could win, changing a setting in the GUI would silently change
what an agent does on its next run, with nothing in the transcript explaining
why.

Open sub-questions:

- Does a board-local settings file exist (per repository), and where does it
  sit in the order?
- Can any setting be marked non-overridable?
- Does `AMD_NO_INPUT` behave as a setting or only as a flag/env?

## Consequences

- Adding a front end means adding a namespace, not changing the library.
- A settings file hand-edited by a user survives a write from either
  interface, key order and all.
- **Cost:** the flat `key = value` format has no types or nesting. It stays
  greppable and hand-editable with no parser dependency, which is worth more
  here than structure.
- The registry's technically-correct XDG home is `$XDG_STATE_HOME` — it is
  state, not configuration. It currently lives in the config directory.
  Moving it needs a fallback read of the old path for a release or two; that
  is a decision to take on purpose, not to leave to a rewrite.
