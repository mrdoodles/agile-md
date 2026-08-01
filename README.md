# agile-md

A tiny **filesystem Kanban** — tasks are markdown files moved between `todo/`,
`doing/` and `done/` directories. Pure `bash` + `git`, no JavaScript, no runtime.

- **Status is the folder.** A task's column is simply which directory it's in.
- **Git is the audit trail.** Moving a task is a `git mv`, so `git log --follow`
  reconstructs exactly when it started and finished.
- **One command, any repo.** Install `task` once on your PATH; it finds the
  board at `<repository-root>/tasks` from wherever you are.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/mrdoodles/agile-md/v2/install.sh | bash
```

Installs the `task` command to `~/.local/bin` (override with
`install.sh --dir /usr/local/bin`). If that directory isn't on your `PATH`, the
installer tells you what to add.

## Use

In any git repository:

```bash
task init                          # scaffold tasks/{todo,doing,done} here
task new "Publish to Marketplace" -t release
task board                         # show all columns (the default)
task start 1                       # todo  -> doing
task done  1                       # doing -> done
task back  1                       # move one column left
task show  publish                 # print a task (id or slug substring)
task edit  1                       # open in $EDITOR
```

`task` works from any subdirectory — it resolves the board from the repo root.
Set `TASKS_DIR` to use a board directory other than `tasks`.

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
