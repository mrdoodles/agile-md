---
id: "012"
title: "create repository board"
ticket: "development"
type: "feat"
assignee: ""
parent: ""
branch: "feature/create-repository-board"
related: []
created: "2026-08-05"
tags: []
---

## Notes
The repository board should display the last repository the user was working with.
A user should be able to select any of the registered repositories from a dropdown.
The repository board should display the tickets for the current repository with tickets in progress owned by the current user at the top.
The repository board should show other tickets assigned to the user and ordered by priority underneath the in progress tickets.
unassigned tickets ordered by priority should be last. 
A create ticket dialog at the top allows the user to create a new ticket in the current repository.
An edit button should allow the editing of an existing ticket.
A move dialog should allow changing the current status of a ticket.

## Acceptance criteria

-  [ ] A user only sees tickets assigned to them or unnasigned.
- [ ] A user only sees tickets for the current repository.
- [ ] A newly created ticket is written out to the currently selected repository.
- [ ] A ticket in the current repository may have its status changed.status 
- [ ] A ticket may have a parent ticket assigned or removed.
- [ ] It should be possible to rename a ticket but not change it's id.
