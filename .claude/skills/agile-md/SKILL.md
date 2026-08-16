---
name: agile-md
description: >-
  Working a filesystem Kanban board managed by the `amd` command — creating,
  moving, archiving and writing tickets under `tasks/{todo,doing,done}`. Load
  whenever asked to raise a ticket, capture work for later, move something to
  doing/done, or edit a task markdown file, in any repo that has a `tasks/`
  board. Covers the process an agent must follow rather than the CLI's
  internals: create with `amd new` (never hand-write filenames), write bodies
  directly but preserve frontmatter, `amd set` for the owned fields, why
  `amd edit` is unusable unattended, and what commits versus what doesn't.
---

# Working an agile-md board

`amd` is a filesystem Kanban: a ticket is a markdown file, its status is which
directory it sits in, and moves are `git mv` so the history is the audit trail.
The board lives at `<repo-root>/tasks/` in whatever repo you are working in.

Run `amd help` first if unsure — it is the authority on the current command set,
which grows. This file is about *process*: the parts that go wrong silently.

## Create tickets with `amd new`, never by hand

```bash
amd new "Validate conventional commits in CI" -p medium -t ci -t lite-actions
```

Writing `tasks/todo/013-something.md` yourself is the single most damaging
shortcut available, because ids must be unique *forever*:

- `next_id` counts `archive/` as well as the live columns. An archived ticket
  keeps its id, so a hand-picked number can collide with one, and every
  existing `[[NNN-slug]]` wikilink to the archived ticket silently starts
  pointing at your new one.
- `amd new` also slugifies the title into the filename and stamps `repository`
  from the `origin` remote. Hand-writing means getting both right by luck.

Never `mkdir` the board either — `amd init`, or `AMD_YES=1` on any command.

## Write the body directly, but keep the frontmatter intact

`amd new` only leaves a skeleton: an empty `## Notes` and a `## Checklist` with
one blank item. There is no command that fills those in, so write the file —
but replace *only* what is below the closing `---`:

```bash
setbody() { f="$1"; head -8 "$f" > /tmp/t && cat >> /tmp/t && mv /tmp/t "$f"; }
setbody tasks/todo/013-slug.md <<'EOF'

## Notes

Why this needs doing, and what is non-obvious about it.

## Checklist

- [ ] First step
EOF
```

Eight lines is the frontmatter (`---`, id, title, repository, priority,
created, tags, `---`). Clobbering it fails *quietly*: `meta()` returns empty
and the ticket lists as `[???]` with no title, at display time, long after the
write. Check afterwards:

```bash
grep -c '^---$' tasks/todo/013-slug.md    # must be 2
```

## Use `amd set` for the two fields `amd` owns

```bash
amd set 13 priority high        # high | medium | low, prefixes work (h/m/l)
amd set 13 repository owner/name
```

Editing those lines by hand skips validation — an invalid priority ranks last
in `-s priority` sorting rather than erroring.

## `amd edit` is unusable unattended

It launches `$MARKDOWN_EDITOR`/`$EDITOR` interactively and blocks. Write the
file directly instead (above). Only suggest `amd edit` to a human.

## Moves are `git mv`, and they do not commit

```bash
amd doing 13     # todo  -> doing
amd done  13     # doing -> done
amd back  13     # one column left
```

For a ticket git already tracks, this is a `git mv`: the rename is staged and
`git log --follow` reconstructs when the work started and finished. For one
that has never been committed it is a plain `mv` and nothing is staged — so a
freshly created ticket you then move leaves no trace until it is committed.

Either way `amd` does not commit. Leave that to the user unless they asked;
if you do commit, the move is the whole point of the diff, so keep it its own
commit.

`<ref>` is an id (`13`, `013`) or a unique slug substring (`amd doing convent`).
It errors on zero or multiple matches, so it is safe to guess.

## Archiving is reversible; cleaning is not

```bash
amd archive 13   # off the board, into tasks/archive/ — still on disk
amd clean        # deletes the whole archive, permanently
```

`archive/` is gitignored, so archiving drops the file from the index and moves
it — the ticket leaves the board *and* the history. Archived tickets stop
resolving as a `<ref>` (`amd show 13` will not find one); `amd ls archive` is
the only view of them.

`amd clean` is the only command in `amd` that destroys anything. It refuses to
run non-interactively unless `AMD_YES=1`, which exists for automation — do not
reach for it just to silence the prompt. Never edit or delete
`tasks/archive/.gitignore`; `amd` recreates it, and without it archived tickets
become committable.

## Finding what to work on

```bash
amd board                          # all columns, ordered by id
amd board -s priority              # high first, ties broken by id
amd board -r lite-actions          # one repository (case-insensitive substring)
amd ls todo -r agile-md -s priority
```

One board can hold work for several repositories; `repository` is what
separates them. Tickets predating a field show `-` and sort last — that is
deliberate, do not "fix" it by backfilling defaults.

## Writing a ticket worth reading

The design principle is that tickets are self-describing: someone opening one
cold gets the context without asking.

- Record **why**, and what is non-obvious — not a restatement of the title.
- Name the blockers and dependencies, and link them: `[[014-other-ticket]]`.
- Absolute dates ("blocked until the 2026-09 release"), never "next week".
- A `## Checklist` of real steps, not one line repeating the title.
- Keep frontmatter facts in frontmatter; do not restate priority in the body.

## Environment

| Variable | Effect |
| --- | --- |
| `AMD_DIR` | Board directory name (default `tasks`) |
| `AMD_YES` | Create the board / confirm `clean` without prompting |
| `AMD_SORT` | Default ordering, `id` or `priority` |
| `MARKDOWN_EDITOR`, `EDITOR` | Used by `amd edit` (interactive only) |

Every board-requiring command needs to be inside a git repository — `amd`
resolves the board from the repo root, so it works from any subdirectory.
