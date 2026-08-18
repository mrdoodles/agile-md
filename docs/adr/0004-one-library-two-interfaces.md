# ADR-0004: One library, two interfaces

- **Status:** Accepted
- **Date:** 2026-08-18

## Context

agile-md has two front ends: a command line and a desktop board. Most people
will use the board; some will use the command line; **AI agents will use the
command line**. They must not diverge — a command that exists in one and not
the other is how a tool stops being trustworthy.

Today the crate is a library plus two binaries, and the separation is real but
held by convention. It has already leaked:

- `board.rs` prints to stdout from the library, so a drag in the GUI writes a
  message nobody reads.
- `render.rs` — `richrs`, `IsTerminal`, OSC-8 escape sequences — is terminal
  code living in the shared library.
- Mutators are split across two types with two vocabularies (`Task::set_*`,
  `Board::set_*`), each independently re-reading and re-writing the file.

Convention did not hold. The boundary needs to be something a compiler checks.

## Decision

A cargo workspace of three crates:

```
crates/
├── amd-lib/    board state, git, templates, groups, registry, settings I/O
├── amd-cli/    clap, inquire, richrs, $EDITOR       → binary `amd`
└── amd-ui/     egui / eframe                        → binary `amdui`
```

**The rule:**

> `amd-lib` owns all persisted state. It is the only crate that reads or
> writes task files, group folders, the registry and the config. It never
> prompts, never prints, and never asks whether a terminal is attached. The
> interfaces translate intent into library calls, and results into pixels or
> text.

Enforcement is the dependency graph: `amd-lib`'s manifest does not list
`clap`, `inquire`, `richrs`, `egui` or `eframe`, so a violation does not
compile.

Two exceptions, named so nobody "fixes" them later:

1. **The CLI's `$EDITOR` temp file.** `amd-cli` writes a scratch file to hand
   a rendered ticket to an editor and reads back what returns. It never
   touches the board — the result comes back as a `String` and goes to the
   library like any other value.
2. **Front-end schema ownership.** `amd-lib` owns the settings *file*;
   `amd-ui` owns the meaning of the keys in its own namespace. See
   [ADR-0005](0005-settings-state-and-precedence.md).

**Binary names do not change.** `amd` and `amdui` are installed, documented
and referenced by `install.sh`. Crate names are an internal matter.

## Consequences

- `render.rs` moves to `amd-cli`. The GUI stops carrying a terminal renderer.
- Library functions that printed now return values, and the caller decides
  whether that becomes a line of text or a toast.
- The GUI cannot write its own theme file — it asks the library to. This is
  the rule working, not the rule being inconvenient.
- **This is an extraction, not a rewrite.** The seams are already in roughly
  the right places; the existing code encodes edge cases that cost real work
  (`deunicode` slugging, the archive's interaction with `.gitignore`,
  SIGPIPE, the `git mv` → `fs::rename` fallback). Moving files preserves that;
  starting again pays for it twice.
- **Cost:** a workspace is more ceremony than a crate — four manifests, and
  cross-crate changes touch more files.

## Alternatives considered

- **Stay a single crate and hold the line by review.** Rejected: that is what
  we have been doing, and the leaks above are the result.
- **Feature flags within one crate.** Rejected: `--no-default-features`
  already exists and did not stop terminal code from landing in the shared
  module. A feature flag gates compilation, not dependencies-by-layer.
