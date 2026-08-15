# agile-md

A tiny **filesystem Kanban** — tasks are markdown files moved between `todo/`,
`doing/` and `done/` directories. Pure `bash` + `git`, no JavaScript, no runtime.

- **Status is the folder.** A task's column is simply which directory it's in.
- **Git is the audit trail.** Moving a task is a `git mv`, so `git log --follow`
  reconstructs exactly when it started and finished.
- **One command, any repo.** Install `amd` once on your PATH; it finds the
  board at `<repository-root>/tasks` from wherever you are.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/mrdoodles/agile-md/v3/install.sh | bash
```

Installs the `amd` command to `/usr/local/bin` when it's writable, otherwise
`~/bin` (override with `install.sh --dir <dir>`). If the chosen directory isn't
on your `PATH`, the installer tells you what to add.

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
amd archive 1                     # off the board, into tasks/archive/
amd clean                         # delete the archive for good
```

`amd` works from any subdirectory — it resolves the board from the repo root.
Set `AMD_DIR` to use a board directory other than `tasks`.

## Archive

Not every task deserves to be `done` — some are abandoned, duplicated, or were
never really tasks. `amd archive <ref>` takes one off the board without
deleting it:

```bash
amd archive 4                     # tasks/todo/004-… -> tasks/archive/004-…
amd ls archive                    # the only view that shows them
amd clean                         # delete everything in archive/, permanently
```

`amd init` creates `tasks/archive/` with a `.gitignore` that ignores the whole
directory, keeping only itself:

```gitignore
*
!.gitignore
```

So the drawer is tracked but its contents never are. Archiving is a plain `mv`
after `git rm --cached` — the task leaves the board *and* the history, which
is the point: `git mv` into an ignored directory would either be refused or
force the archive back into the repository the `.gitignore` exists to keep it
out of. Your commit of the archive shows only the deletion from the column.

Two things archiving deliberately does not do: archived ids are never reused
(`amd new` counts the archive too, so old `[[NNN-slug]]` references can't be
silently repointed at a different task), and archived tasks stop resolving as
a `<ref>` — `amd show 4` won't find one. `amd clean` is the only command in
`amd` that deletes anything, and it refuses to run non-interactively unless
you set `AMD_YES=1`.

If you run a command in a repo that has no board yet, `amd` offers to create
one for you (interactively). In non-interactive use it errors instead of
hanging; set `AMD_YES=1` to create the board without prompting.

## Task format

Each task is `tasks/todo/NNN-slug.md` with light frontmatter:

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
