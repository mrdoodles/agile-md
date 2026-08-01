# agile-md

A tiny **filesystem Kanban** — tasks are markdown files moved between `todo/`,
`doing/` and `done/` directories. Pure `bash` + `git`, no JavaScript, no runtime,
nothing to install globally.

- **Status is the folder.** A task's column is simply which directory it's in.
- **Git is the audit trail.** Moving a task is a `git mv`, so `git log --follow`
  reconstructs exactly when it started and finished.
- **Self-contained.** The `task` script is vendored into each board, so it works
  on clone with zero dependencies.

## Install into a repository

From the repo you want a board in:

```bash
curl -fsSL https://raw.githubusercontent.com/mrdoodles/agile-md/v1/install.sh | bash
```

Or clone and run `./install.sh [board-dir]` (default board dir: `tasks`). This
creates `tasks/{todo,doing,done}/` and vendors the `task` script into `tasks/`.

## Usage

```bash
tasks/task new "Publish to the Marketplace" -t release   # create in todo/
tasks/task board                                         # show all columns
tasks/task start 1                                       # todo  -> doing
tasks/task done  1                                       # doing -> done
tasks/task back  1                                       # move one column left
tasks/task show  1                                       # print a task
tasks/task edit  publish                                 # open in $EDITOR
```

`<ref>` is a task id (e.g. `7` or `007`) or a unique slug substring
(e.g. `publish`).

## Task format

Each task is `todo/NNN-slug.md` with light frontmatter:

```markdown
---
id: "001"
title: "Publish to the Marketplace"
created: "2026-08-01"
tags: [release]
---

## Notes


## Checklist

- [ ]
```

The `NNN` id is assigned in creation order (across all columns) and also gives a
stable default ordering. Commit task moves like any other change — the `git mv`
is the record of the transition.

## Why a directory instead of one big TODO.md?

A single file gets unwieldy once tasks have real content, and every edit is a
merge conflict magnet. Separate files keep tasks self-describing, diffable and
independently movable, while the folders give an unambiguous board.
