# agile-md

A tiny **filesystem Kanban** — tasks are markdown files moved between `backlog/`,
`todo/`, `doing/` and `done/` directories. Pure `bash` + `git`, no JavaScript,
no runtime.

- **Status is the folder.** A task's column is simply which directory it's in.
- **Git is the audit trail.** Moving a task is a `git mv`, so `git log --follow`
  reconstructs exactly when it started and finished.
- **One command, any repo.** Install `amd` once on your PATH; it finds the
  board at `<repository-root>/tasks` from wherever you are.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/mrdoodles/agile-md/v5/install.sh | bash
```

Installs the `amd` command to `/usr/local/bin` when it's writable, otherwise
`~/bin` (override with `install.sh --dir <dir>`). If the chosen directory isn't
on your `PATH`, the installer tells you what to add.

### Claude Code skill (optional)

If you use [Claude Code](https://claude.com/claude-code), `--with-skill` also
installs a skill that teaches it to drive a board properly — creating tickets
with `amd new` rather than hand-writing files (which can silently reuse an
archived id), keeping frontmatter intact when writing a ticket body, and
leaving the commit to you:

```bash
./install.sh --with-skill      # -> ~/.claude/skills/agile-md/SKILL.md
```

It's off by default: installing `amd` shouldn't write into another tool's
configuration. To scope it to one project instead, copy the same file to
`.claude/skills/agile-md/SKILL.md` in that repo.

## Use

In any git repository:

```bash
amd init                          # scaffold tasks/{backlog,todo,doing,done} here
amd new "Publish to Marketplace" -t release -p high
amd board                         # show all columns (the default)
amd todo  1                       # backlog -> todo (pull into the sprint)
amd doing 1                       # todo  -> doing
amd done  1                       # doing -> done
amd back  1                       # move one column left
amd show  publish                 # print a task (id or slug substring)
amd edit  1                       # open in $MARKDOWN_EDITOR
amd set   1 priority low          # change a field on an existing task
amd archive 1                     # off the board, into tasks/archive/
amd clean                         # delete the archive for good
```

`amd` works from any subdirectory — it resolves the board from the repo root.
Set `AMD_DIR` to use a board directory other than `tasks`.

Tasks are markdown, so `amd edit` prefers `$MARKDOWN_EDITOR` over `$EDITOR`,
and falls back to `vi`:

```bash
export MARKDOWN_EDITOR="obsidian"   # or "code --wait", "typora", …
```

A candidate is skipped when it's unset, empty, or names a command this machine
hasn't got — so `MARKDOWN_EDITOR` set in a dotfile you share between machines
quietly falls back to `$EDITOR` on the ones without it. Arguments are allowed;
only the command itself has to exist.

## Backlog, refinement and sprints

New tasks land in `backlog/`, not `todo/`. The backlog is everything you might
do; `todo/` is what you've committed to doing now. Refinement is moving a task
between them:

```bash
amd new "Fix the release job"     # -> backlog/, unrefined
amd ls backlog -s priority        # what's worth pulling next
amd todo 7                        # pull it into the sprint
amd back 7                        # changed your mind — back to the backlog
```

That keeps `amd board` honest. Without a backlog, `todo/` collects every idea
anyone ever had and stops meaning anything; with one, the size of `todo/` is
the size of your commitment.

Nothing forces you to use it — `amd doing 7` straight from the backlog works
if a task needs no refinement.

## Priority and repository

Every task records a `priority` (`high`, `medium` or `low` — default `medium`)
and the `repository` it belongs to. The repository defaults to the one you
created the task in (`owner/name`, read from the `origin` remote), so a single
board can track work across several repositories and filter it back apart:

```bash
amd new "Fix the release job" -p high -r mrdoodles/lite-actions
amd set 7 priority low            # or: amd set 7 repository mrdoodles/agile-md
amd board -s priority             # highest first, ties broken by id
amd board -r lite-actions         # only that repository (substring, any case)
amd ls backlog -r lite-actions -s priority
```

Ordering is by id unless you ask for `-s priority`; set `AMD_SORT=priority` to
make that the default. Tasks written before these fields existed keep working —
they show a `-` for priority, sort last under `-s priority`, and `amd set` adds
the missing field to the frontmatter.

## Archive

Not every task deserves to be `done` — some are abandoned, duplicated, or were
never really tasks. `amd archive <ref>` takes one off the board without
deleting it:

```bash
amd archive 4                     # tasks/backlog/004-… -> tasks/archive/004-…
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
(ids come from `tasks/.next-id`, and `amd new` counts the archive too, so old
`[[NNN-slug]]` references can't be silently repointed at a different task),
and archived tasks stop resolving as
a `<ref>` — `amd show 4` won't find one. `amd clean` is the only command in
`amd` that deletes anything, and it refuses to run non-interactively unless
you set `AMD_YES=1`.

If you run a command in a repo that has no board yet, `amd` offers to create
one for you (interactively). In non-interactive use it errors instead of
hanging; set `AMD_YES=1` to create the board without prompting.

## Task format

Each task is `tasks/<column>/NNN-slug.md` — the `NNN` id and the slug come from
the title, the column is the directory. This is exactly what

```bash
amd new "Publish to Marketplace" -t release -p high
```

writes to `tasks/backlog/001-publish-to-marketplace.md`:

```markdown
---
id: "001"
title: "Publish to Marketplace"
repository: "mrdoodles/agile-md"
priority: "high"
created: "2026-08-13"
tags: [release]
---

## Notes


## Checklist

- [ ]
```

| Field | Set by | Notes |
| --- | --- | --- |
| `id` | `amd new` | Zero-padded and never reused, tracked in `tasks/.next-id`. Never changes — it's how `<ref>` and `[[NNN-slug]]` wikilinks find a task. |
| `title` | `amd new "<title>"` | Verbatim; also slugified into the filename. |
| `repository` | `-r`, or the `origin` remote | `owner/name` from `git remote get-url origin`, falling back to the repository directory's name. What `amd board -r` filters on. |
| `priority` | `-p`, default `medium` | `high`, `medium` or `low`. What `amd board -s priority` orders by. |
| `created` | `amd new` | `YYYY-MM-DD`, the day it was created. |
| `tags` | `-t` (repeatable) | Comma-separated inside `[]`, e.g. `tags: [release,ci]`; empty as `tags: []`. |

`amd set <ref> priority|repository <value>` rewrites those two fields in place;
everything else is yours to edit — `amd edit <ref>` opens the file in your editor,
and the `## Notes` / `## Checklist` headings are only a starting point.

Nothing outside the frontmatter is parsed, so a task can grow whatever body it
needs. Missing fields are tolerated: tasks written before `repository` and
`priority` existed still list and sort (last, under `-s priority`), and
`amd set` adds the field when it isn't there.

Ids come from `tasks/.next-id`, a one-line counter in the board, taken
together with the highest id on disk — whichever is higher wins. That way an id
is never reused even if you delete a task outright, and deleting the counter
costs you nothing but those freed ids. Commit it along with the board.

Boards created before the counter existed need no migration: the first `amd`
command of any kind seeds it from the highest id already on the board.

The id gives a stable default ordering. Commit task moves like any other change
— the `git mv` is the record of the transition. Tasks can reference each other
with `[[NNN-slug]]` wikilinks.

## Why a directory instead of one big TODO.md?

A single file gets unwieldy once tasks have real content, and every edit is a
merge-conflict magnet. Separate files keep tasks self-describing, diffable and
independently movable, while the folders give an unambiguous board.
