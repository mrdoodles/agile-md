---
id: "013"
title: "ticket-fields"
ticket: "development"
type: "refactor"
assignee: ""
parent: ""
branch: "chore/ticket-fields"
related: []
created: "2026-08-05"
tags: []
---
A ticket  needs to be updated.
Required fields:
id - mandatory auto-generated
title - mandatory - type string
branch-type - optional - conventional branch type dropdown 
branch-name - optional - auto-generated from branch-type and ticket title i.e. chore/ticket-fields
parent - optional - a ticket id in the same repository
related - optional - a list of related ticket id's in the same repository
tags - optional comma separated list of strings
created - auto-generated date of ticket creation.
## Notes
Need to find a good way to display these details as a form

## Acceptance criteria

- [ x ]  the required fields above appear in the ticket form .
