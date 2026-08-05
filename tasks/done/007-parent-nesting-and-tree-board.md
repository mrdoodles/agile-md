---
id: "007"
title: "parent nesting and tree board"
ticket: "development"
type: "feat"
parent: ""
branch: "feature/parent-nesting-and-tree-board"
related: []
created: "2026-08-04"
tags: []
---

## Notes

Replace the epic and story fields with a single `parent`. Nesting gives both
levels and any depth past them, and nothing has to be decided up front about
which level a ticket belongs to. Show the result as a tree on the board.

## Acceptance criteria

- [x] A ticket can sit under another with `--parent <ref>`
- [x] The child links to the parent and the parent lists its children
- [x] The board draws each column as a tree, ordered by id at every level
- [x] Ids on the board open the ticket when clicked
- [x] A child whose parent is in another column stays visible with a marker
- [x] Piped output is the same tree as plain indented text
