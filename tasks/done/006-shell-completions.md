---
id: "006"
title: "shell completions"
ticket: "development"
type: "feat"
parent: ""
branch: "feature/shell-completions"
related: []
created: "2026-08-04"
tags: []
---

## Notes

Completion for bash, zsh and fish, generated from the CLI itself so it cannot
drift from it.

Delivered alongside [[003-link-related-tickets]].

## Acceptance criteria

- [x] `amd completions <shell>` prints a script for bash, zsh and fish
- [x] With no argument the shell is taken from $SHELL
- [x] Completion covers values, not just command and flag names
- [x] The change types offered include anything added with AMD_TYPES
- [x] The script goes to stdout and the install hint to stderr
- [x] It works outside a board, which is when completions get set up
