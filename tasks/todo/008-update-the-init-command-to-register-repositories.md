---
id: "008"
title: "update the init command to register repositories"
ticket: "development"
type: "refactor"
assignee: ""
parent: ""
branch: "chore/update-the-init-command-to-register-repositories"
related: []
created: "2026-08-05"
tags: []
---

## Notes
When a repository gets registered its name and path get added to the repositories list in the database.
This enables:
- switching between repositories in the repository view.
- seeing a consolidated list of open tickets in the work view.

## Acceptance criteria

 - [ ] when a repository is registered it can be seen in the database.
- [ ] both the repository name along with its path exist in the database.
