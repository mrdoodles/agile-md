# ADR-0001: Status is the folder

- **Status:** Accepted
- **Date:** 2026-08-18

## Context

A kanban board has to record which column each ticket is in. The obvious
options are a field in the ticket, an index file, or the location of the file
itself.

agile-md is a filesystem tool: the board is a directory of markdown in the
repository being worked on, and it must stay legible to `ls`, `grep`, an
editor, a code review and anyone who has never installed `amd`.

## Decision

**A ticket's column is the directory it sits in.** `backlog/`, `todo/`,
`doing/`, `done/`. There is no `status` key in the frontmatter. Changing a
ticket's status is moving the file, and nothing else.

`archive/` is deliberately **not** a column. It is off the board, its
contents are gitignored, and nothing lists it.

## Consequences

- The board is readable with `ls`, and a column is a directory listing. No
  index can go stale, because there is no index.
- A move is one filesystem operation. There is no window in which the file
  says one thing and the board says another.
- Two sources of truth cannot disagree, because there is only one.
- Renaming or adding a column is a directory operation.
- **Cost:** a ticket carries no record of its own status. A ticket file sent
  to someone out of context loses it. This is accepted: the board is the
  directory, and a ticket outside a board is a document, not a ticket.
- **Cost:** a ticket cannot be in two columns at once. This is a feature.

## Alternatives considered

- **A `status:` key in the frontmatter.** Rejected: the file and its location
  can then disagree, and something has to arbitrate. Every tool that has tried
  this ends up with a repair command.
- **An index file or database.** Rejected: it is stale the moment someone
  checks out a branch, pulls, or moves a file by hand — all of which are
  ordinary things to do in a repository. Reading a board is a directory
  listing and a few small files; an index buys nothing and costs correctness.
