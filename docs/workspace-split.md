# The workspace split — migration plan

How [ADR-0004](adr/0004-one-library-two-interfaces.md) gets implemented without
a rewrite. This is a working document, not a decision record; it is finished
when the split is done.

## Target

```
agile-md/
├── Cargo.toml              workspace
├── crates/
│   ├── amd-lib/            board state, git, templates, groups, registry, settings I/O
│   ├── amd-cli/            → binary `amd`
│   └── amd-ui/             → binary `amdui`
├── docs/adr/
├── templates/
└── tests/test.sh
```

Binary names are unchanged. `install.sh` and the release URL keep working.

## Where each file lands

| Today | Crate | Note |
|---|---|---|
| `board.rs`, `task.rs`, `create.rs`, `group.rs`, `git.rs`, `branch.rs`, `templates.rs`, `registry.rs`, `settings.rs` | **amd-lib** | `board.rs:513`'s `println!` becomes a returned value |
| `render.rs` | **amd-cli** | terminal renderer: `richrs`, `IsTerminal`, OSC-8 |
| `form.rs`, `value.rs`, `completions.rs`, `main.rs` | **amd-cli** | already CLI-only |
| `gui/mod.rs`, `gui/richtext.rs` | **amd-ui** | |
| `gui/settings.rs` | **amd-ui** | schema only; the file I/O stays in `amd-lib` ([ADR-0005](adr/0005-settings-state-and-precedence.md)) |
| `templates/*.md.jinja` | **amd-lib** | `include_str!`'d there |

`amd-lib`'s manifest must not list `clap`, `inquire`, `richrs`, `egui` or
`eframe`. That absence is the enforcement.

## Order of work

Each phase ends with a green build, green `cargo test`, green `tests/test.sh`.

**Phase 0 — make the spec green.** `tests/test.sh` has 29 failing assertions:
it asserts `tasks/todo/001-first-task.md` while `amd new` now lands in
`backlog/`. This is a precondition, not a nicety — the suite is the only
safety net for moving 5,000 lines, and a red suite cannot tell us whether the
move broke something.

**Phase 1 — fix the stale surfaces.** The obsolete "two ticket types"
help text in `main.rs`, the `# GUI only` comment above `clap_complete` in
`Cargo.toml`, and the TUI reference in `settings.rs`. Cheap, and it stops the
next reader inheriting the confusion.

**Phase 2 — workspace, one crate.** Introduce the workspace with `amd-lib`
only; the binaries stay where they are and depend on it. Nothing moves yet.

**Phase 3 — extract `amd-cli`.** Move the CLI modules and `render.rs`. This is
where the library stops printing.

**Phase 4 — extract `amd-ui`.** Move the GUI, splitting `gui/settings.rs` into
schema (`amd-ui`) and storage (`amd-lib`).

**Phase 5 — the operation vocabulary.** One write per action, typed failures,
`--json`, the parity test ([ADR-0006](adr/0006-the-operation-vocabulary.md)).
Identity becomes a parameter here rather than something the library reads
from `git config` ([ADR-0008](adr/0008-room-for-a-server.md)) — doing it with
the signatures rather than after them is the whole point.

**Phase 6 — concurrency.** Optimistic writes first; leases after
([ADR-0007](adr/0007-concurrency-and-locking.md)).

Phases 0–4 are mechanical and should not change behaviour. Phases 5–6 change
behaviour and each want their own review.

## Known drift to resolve on the way

- `amd ls` sorts by id and ignores `order`; the board sorts by `order`.
  Resolved by one comparator in the library — `(priority, order, id)`.
- `priority` does not exist yet. Adding it is a value parser, a frontmatter
  key and a template line; it is a new field, not a migration.
- `amd board` shows one repository; the desktop board shows all registered
  ones. There is no `amd board --all`.
- CI runs `cargo clippy --all-targets --all-features -D warnings`, so every
  crate in the workspace must stay clippy-clean, not just the one being
  worked on.
