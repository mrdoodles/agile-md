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
- **The template is the form.** Leave an argument out in a terminal and `amd`
  asks — including the fields your own template declares.
- **Two ticket types.** Development work is tracked on a branch named from its
  conventional-commit type — `amd start` puts you on `feature/add-login`. Admin
  work gets no branch. Both take optional `epic` and `story` labels.
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
amd new "Publish to Marketplace" --type feat --epic launch
amd board                         # show all columns (the default)
amd start 1                       # todo -> doing, and switch to feature/publish-to-marketplace
amd done  1                       # doing -> done
amd back  1                       # move one column left
amd show  publish                 # print a task (id or slug substring)
amd edit  1                       # open in $EDITOR
amd epics                         # epics with progress; amd stories does the same
```

`amd` works from any subdirectory — it resolves the board from the repo root.
Set `AMD_DIR` to use a board directory other than `tasks`.

## Ticket types

Two kinds of ticket, each a template:

| Ticket type   | Branch?                              | Frontmatter                        |
| ------------- | ------------------------------------ | ---------------------------------- |
| `development` | yes — `<prefix>/<title>`             | `type`, `branch`, epic, story, tags |
| `admin`       | no — a rota or a renewal has nothing to check out | epic, story, tags     |

```bash
amd new "Add login"                          # development, the default
amd new "Renew the certificates" -T admin    # admin
amd start 1                                  # development: switches to feature/add-login
amd start 2                                  # admin: "admin tickets don't use branches"
```

The rule lives in the template, not in a flag: a ticket type whose template
records a `branch` gets one. Your own template opts in the same way, just by
using the variable.

## Labels

On top of the ticket type:

| Label   | On            | Values                                                    |
| ------- | ------------- | --------------------------------------------------------- |
| `type`  | development   | conventional-commit types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert` (default `feat`; the list is `AMD_TYPES`) |
| `epic`  | both          | optional — groups tasks across a body of work (`amd epics`) |
| `story` | both          | optional — groups tasks within an epic (`amd stories`)      |
| `tags`  | both          | free-form, completed against the tags already on the board  |

The change type **names** the branch. Ticket types are *commit* types, branches
follow the *branch* convention, and `amd` maps between them — so both halves
satisfy [conventional-validator](https://github.com/mrdoodles/conventional-validator):

| change type                                     | branch prefix |
| ----------------------------------------------- | ------------- |
| `feat`                                          | `feature/`    |
| `fix`                                           | `bugfix/`     |
| `revert`                                        | `hotfix/`     |
| `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore` | `chore/`      |

```bash
amd new "Crash on save" --type fix     # created todo/003-crash-on-save.md (bugfix/crash-on-save)
amd start 3 --branch spike/try-it      # …or a branch you name
amd start 3 --no-branch                # …or none at all (AMD_NO_BRANCH=1 for good)
```

The branch name is written into the ticket at creation, so you can see it — and
edit it — before any work starts. Because the title becomes a branch, it's
**validated when you type it**: a title with nothing sluggable in it is rejected
rather than producing a ticket that can never start.

## Forms

Every argument is optional in a terminal. Leave it out and you get a prompt
([inquire](https://github.com/mikaelmello/inquire)) instead of a usage error:

```bash
amd new                # title, ticket type, change, epic, story, tags, then the body in $EDITOR
amd new "Ship it" -i   # same form, pre-filled with what you passed
amd new "Ship it" -e   # straight to the body editor
amd new "Ship it"      # one-liner, no editor (--no-edit forces this)
amd start              # pick from the tasks in todo/
amd done               # pick from the tasks in doing/
amd show / amd edit    # pick from the whole board
```

The form doesn't stop at the metadata: it finishes by opening the rendered
ticket in `$EDITOR` — like `git commit` — so the notes and checklist are filled
in while the task is being created. Nothing is written to the board until the
editor exits cleanly, so quitting with a non-zero status (`:cq` in vim) or
emptying the file creates nothing.

Nothing prompts unless both ends are a terminal, so scripts and CI behave
exactly as before — a missing value is an error, never a hang. `--no-input`
(or `AMD_NO_INPUT=1`) forces that behaviour in a terminal too.

If you run a command in a repo that has no board yet, `amd` offers to create
one for you (interactively). In non-interactive use it errors instead of
hanging; set `AMD_YES=1` to create the board without prompting.

## Templates

Every task file is rendered from a MiniJinja template — nothing is assembled by
hand, so a ticket can't come out half-formatted:

```bash
amd templates                     # what's available, and where it comes from
amd templates show development    # print the source
amd templates eject development   # copy it to tasks/templates/ to edit
amd new "Renew the certs" -T admin # use a different ticket type
amd new "Ship it" -s owner=tim    # extra variables, as extra.owner
```

Anything in `<board>/templates/<name>.md.jinja` overrides a built-in of the same
name or adds a new one, so a repo can carry its own ticket format with no tool
changes. Built-ins: `development`, `admin`, and `board-readme` (used by
`amd init`).

**A template that uses `extra.<name>` gets asked for it.** There's no second
place to register fields — write this in `tasks/templates/story.md.jinja`:

```jinja
owner: {{ extra.owner | yaml }}

## Acceptance criteria

{{ extra.acceptance_criteria }}
```

…and `amd new "Fast board" -T story` asks "Owner:" and "Acceptance criteria:".
Non-interactively the same fields must be supplied
(`-s owner=tim -s acceptance_criteria=…`) or the command fails saying which are
missing — so a half-filled ticket can't be created either way.

Variables available in a task template:

| Variable    | Example                     |
| ----------- | --------------------------- |
| `id`        | `007` (zero-padded)         |
| `number`    | `7`                         |
| `title`     | `Publish to Marketplace`    |
| `slug`      | `publish-to-marketplace`    |
| `type`      | `feat` (empty on admin)     |
| `epic`      | `launch` (or empty)         |
| `story`     | `guest-checkout` (or empty) |
| `branch`    | `feature/publish-to-marketplace` |
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
ticket: "development"
type: "feat"
epic: "launch"
story: ""
branch: "feature/publish-to-marketplace"
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

## Shell completion

```bash
amd completions bash > /usr/local/etc/bash_completion.d/amd
amd completions zsh  > "${fpath[1]}/_amd"          # then: rm -f ~/.zcompdump; compinit
amd completions fish > ~/.config/fish/completions/amd.fish
```

`amd completions` with no argument works the shell out from `$SHELL`; elvish and
powershell are available too. The script goes to stdout and the install hint to
stderr, so redirecting gives you a clean file.

Completion covers values, not just names — `amd new --type <TAB>` offers the
change types this board accepts (including anything you added with `AMD_TYPES`),
`-T <TAB>` the ticket types, `amd ls <TAB>` the columns. Scripts are generated
from the CLI itself, so they can't drift from it.

## The board

In a terminal `amd board` draws with [richrs](https://crates.io/crates/richrs) —
bordered tables, a colour per column, counts and labels:

```
                         TODO  (2)
┌─────┬──────────────────┬────────────────────────────────┐
│ id  │ task             │ labels                         │
├─────┼──────────────────┼────────────────────────────────┤
│ 002 │ Crash on save    │ fix epic:checkout              │
│ 003 │ Update the guide │ docs                           │
└─────┴──────────────────┴────────────────────────────────┘
```

Piped or redirected output stays plain text (`  [002] Crash on save  (fix …)`),
so `amd board | grep` keeps working and log files don't fill with box-drawing
characters. `--plain` or `NO_COLOR=1` forces that everywhere.

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
