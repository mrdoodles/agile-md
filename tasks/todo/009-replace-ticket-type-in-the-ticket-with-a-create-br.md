---
id: "009"
title: "replace ticket type in the ticket with a branch toggle"
ticket: "development"
type: "refactor"
assignee: ""
parent: ""
branch: "chore/change-ticket-type-to-branch-toggle"
related: []
created: "2026-08-05"
tags: []
---

## Notes
The ticket type list could grow and was added to enable auto branch creation.
This can be simplified to a simple "branch" checkbox with a default of on.

## Acceptance criteria

- [ ]  when the "branch" checkbox is selected the branch gets automatically created when the ticket is moved to doing.
- [ ] when the cranch checkbox is not selected then no branch is created when the task moves to doing.
