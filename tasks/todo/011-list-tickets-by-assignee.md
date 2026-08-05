---
id: "011"
title: "list tickets by assignee"
ticket: "development"
type: "feat"
assignee: ""
parent: ""
branch: "feature/list-tickets-by-assignee "
related: []
created: "2026-08-05"
tags: []
---

## Notes

There should be a tab that shows all the tickets assigned to a signee. The
default should be to show the tickets assigned to the current assignee. It
should be possible to select a different user and display tickets assigned to
that user. It should be possible display all unnasigned tickets.

## Acceptance criteria

- [ ] when a user enters the tickets tab they should see tickets assigned to
      them across all the repositories.
- [ ] Tickets should be ordered by priority with the highest priority at the
      top.
- [ ] Tickets in progress should be listed higher than other tickets of the same
      priority.
- [ ] When a user selects an assignee from the assignee dropdown they should see
      tickets for that assignee.
- [ ] When a user selects unassigned from the assignee selection dropdown they
      should see all the unassigned tickets.
