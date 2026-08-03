# agile-md

A tiny **filesystem Kanban** — tasks are markdown files moved between `todo/`,
`doing/` and `done/` directories. A single static binary plus `git`; no runtime,
no daemon, no database.

- **Status is the folder.** A task's column is simply which directory it's in.
- **Git is the audit trail.** Moving a task is a `git mv`, so `git log --follow`
  reconstructs exactly when it started and finished.
- **Tickets come from templates.** Task files are rendered from
  [MiniJinja](https://github.com/mitsuhiko/minijinja) templates, so the format
  is consistent by construction and yours to change.
- **One command, any repo.** Install `amd` once on your PATH; it finds the
  board at `<repository-root>/tasks` from wherever you are.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/mrdoodles/agile-md/v4/install.sh | bash
```

Downloads the prebuilt binary for your platform (Linux/macOS, x86_64/aarch64)
into `/usr/local/bin` when it's writable, otherwise `~/bin`. Override with
`install.sh --dir <dir>`, pin with `--version vX.Y.Z`, or build locally with
`--from-source` (needs [Rust](https://rustup.rs)). From a clone:

```bash
cargo build --release        # target/release/amd
./install.sh                 # builds and installs
```

## Use

In any git repository:

```bash
amd init                          # scaffold tasks/{todo,doing,done} here
amd new "Publish to Marketplace" -t release
amd board                         # show all columns (the default)
amd start 1                       # todo  -> doing
amd done  1                       # doing -> done
amd back  1                       # move one column left
amd show  publish                 # print a task (id or slug substring)
amd edit  1                       # open in $EDITOR
```

`amd` works from any subdirectory — it resolves the board from the repo root.
Set `AMD_DIR` to use a board directory other than `tasks`.

If you run a command in a repo that has no board yet, `amd` offers to create
one for you (interactively). In non-interactive use it errors instead of
hanging; set `AMD_YES=1` to create the board without prompting.

## Templates

Every task file is rendered from a MiniJinja template — nothing is assembled by
hand, so a ticket can't come out half-formatted:

```bash
amd templates                     # what's available, and where it comes from
amd templates show task           # print the source
amd templates eject task          # copy it to tasks/templates/ to edit
amd new "Crash on save" -T bug    # use a different template
amd new "Ship it" -s owner=tim    # extra variables, as extra.owner
```

Anything in `<board>/templates/<name>.md.jinja` overrides a built-in of the same
name or adds a new one, so a repo can carry its own ticket format with no tool
changes. Built-ins: `task`, `bug`, and `board-readme` (used by `amd init`).

Variables available in a task template:

| Variable    | Example                     |
| ----------- | --------------------------- |
| `id`        | `007` (zero-padded)         |
| `number`    | `7`                         |
| `title`     | `Publish to Marketplace`    |
| `slug`      | `publish-to-marketplace`    |
| `tags`      | `["release"]`               |
| `created`   | `2026-08-03`                |
| `timestamp` | `2026-08-03T09:12:44+01:00` |
| `column`    | `todo`                      |
| `author`    | `git config user.name`      |
| `email`     | `git config user.email`     |
| `board`     | `tasks`                     |
| `template`  | `task`                      |
| `extra`     | whatever you passed to `-s` |

The `yaml` filter quotes and escapes a value for frontmatter (`title: {{ title
| yaml }}`) — a quote or backslash in a title can't corrupt the block.
Undefined variables are a **hard error**, so a typo in a template fails loudly
instead of silently rendering a blank line.

## Task format

Each task is `tasks/todo/NNN-slug.md`, from the built-in `task` template:

```markdown
---
id: "001"
title: "Publish to Marketplace"
created: "2026-08-01"
tags: [release]
---

## Notes


## Checklist

- [ ]
```

The `NNN` id is assigned in creation order and gives a stable default ordering.
Commit task moves like any other change — the `git mv` is the record of the
transition. Tasks can reference each other with `[[NNN-slug]]` wikilinks.

## Why a directory instead of one big TODO.md?

A single file gets unwieldy once tasks have real content, and every edit is a
merge-conflict magnet. Separate files keep tasks self-describing, diffable and
independently movable, while the folders give an unambiguous board.

## Development

```bash
cargo test                    # unit tests (slugify, frontmatter, templates)
bash tests/test.sh            # end-to-end CLI spec against throwaway git repos
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all --check
```

`amd` was pure bash through v3; v4 is the same tool in Rust, which is what
buys the templating. The CLI, the board layout and the file format are
unchanged — v3 boards work as-is.
