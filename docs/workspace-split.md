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

**Phase 0 — make the spec green. ✅ done.** 136 passing, 0 failing. The 29
failures were stale assertions, not regressions: `amd new` lands in `backlog/`
and the junk drawer became `archive/`. `amd set` and `amd group` had no
coverage at all and now have 17 assertions, verified by mutation.

**Phase 1 — fix the stale surfaces. ✅ done.** The obsolete "two ticket types"
help text, `AMD_TYPES` documented in `--help` long after nothing read it, the
`# GUI only` comment above `clap_complete`, and the TUI references. Two unit
tests were pinned to the wrong documentation and asserted on it; they now
assert on values a parser actually advertises.

**Phase 2 — the workspace. ✅ done.** All three crates exist and every file is
under one of them, moved with `git mv` so history follows. `amd-lib` keeps the
crate name `agile_md`, so no `use` in the workspace changed. Two things stayed
behind deliberately, because phase 2 is mechanical and neither is:

- `render.rs` is still in `amd-lib` — moving it is phase 3, along with the
  library's `println!`s.
- `gui/` is still in `amd-lib` behind the `gui` feature (off by default, so
  `amd-cli` pulls no windowing stack) — moving it is phase 4.

`amd-cli` therefore keeps a `gui` feature, on by default, so that `amd gui`
behaves exactly as it did. Making it exec the `amdui` binary instead would take
egui out of `amd-cli` entirely, but a prebuilt install ships only `amd`, so
that trade belongs to packaging rather than to a mechanical phase.

**Note:** `cargo build --no-default-features` at the workspace root no longer
produces a CLI-only build — `amd-ui` requires `agile-md/gui`, and feature
unification turns it back on. The headless build is now
`cargo build -p amd-cli --no-default-features`. README and CLAUDE.md say so.

**Phase 3 — `render.rs` to `amd-cli`. ✅ done.** The library is silent: no
`println!`, no `IsTerminal`, no process-global state outside `gui/`, and
`richrs` is gone from its manifest. `Board::move_task` returns the destination
and `amd` prints `moved X -> done/` itself, which is now pinned by the suite.

Moving `render.rs` out of the library also revealed `flatten()` as dead —
written for the TUI's list widget, unreferenced since `cc713f7` removed it, and
invisible while `pub` in a library made "unused" unprovable.

**Phase 4 — `gui/` to `amd-ui`.** Splitting `gui/settings.rs` into schema
(`amd-ui`) and storage (`amd-lib`), after which `amd-lib` lists no `egui` at
all and the rule in ADR-0004 is fully enforced by the manifest.

**Phase 5 — the operation vocabulary.** One write per action, typed failures,
`--json`, the parity test ([ADR-0006](adr/0006-the-operation-vocabulary.md)).
Identity becomes a parameter here rather than something the library reads
from `git config` ([ADR-0008](adr/0008-room-for-a-server.md)) — doing it with
the signatures rather than after them is the whole point.

**Phase 6 — concurrency.** Optimistic writes first; leases after
([ADR-0007](adr/0007-concurrency-and-locking.md)).

Phases 0–4 are mechanical and should not change behaviour. Phases 5–6 change
behaviour and each want their own review.

## Packaging — settled before phase 4

`rust-release` packaged one binary, so a release could ship `amd` or `amdui`
but not both, and `amd gui` was the only way into the board for anyone who had
not built from source. That constrained phase 4: taking egui out of `amd-cli`
means `amd gui` has to exec `amdui`, which only works if `amdui` is installed.

`bin-name` now takes a list (mrdoodles/rust-release), and this repo passes
`"amd amdui"`. Both binaries go into one zip, named after the first, so the
download URL install.sh has always used is unchanged. **The v1 tag has to move
before a release picks this up** — release.yml pins `@v1`.

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
