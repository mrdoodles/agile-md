---
id: "026"
title: "Engineering notes on a ticket, exported as a findable set"
assignee: ""
branch-type: "feature"
branch-name: "feature/engineering-notes-on-a-ticket-exported-as-a-findab"
parent: ""
related: [027]
tags: [docs,knowledge]
created: "2026-08-20"
---

## Notes

Someone hits a problem mid-ticket, works it out, and the working-out is lost:
it lives in their head, or in a Slack thread, or nowhere. The moment the answer
is freshest is while the ticket is open.

Two halves, and only the first is settled.

### The section (do this first — it needs no code)

Add `## Engineering notes` to `ticket.md.jinja`. The template *is* the form
(ADR-0003), so this is the whole feature for the capture half — any board can
have it today with `amd templates eject ticket`.

Use `### <heading>` per finding inside the section, so one ticket can yield
several notes and each carries a title written when the thought was fresh. The
ticket title will not do: "Fix login" tells a future reader nothing, and the
useful title names the *finding* — "wgpu surface lost after macOS sleep".

### The export (only if search does not make it unnecessary)

The original idea was to write the notes out to `engineering/` when the ticket
is closed. Three objections:

- **It is a copy, and copies drift.** The ticket already holds the notes and
  lives in `done/` forever, tracked. Two files with the same words disagree the
  moment either is edited, with nothing to say which is right. This codebase
  has refused that repeatedly — no `reporter`, no `moved-at` (ADR-0002).
- **"On close" is the wrong trigger.** Notes matter most while someone is
  stuck. And tickets that are archived or abandoned often hold the most
  valuable "we tried this and it did not work" — a close-triggered export loses
  exactly those.
- **The value is findability, not storage.** Ticket files are named for the
  task, so what you learned is filed under a name that does not describe it.

If an export happens, it must be **derived and regenerable** — `amd notes
export` rebuilding the directory from the tickets every run, a build artifact
rather than a second source. Then drift is impossible and it covers tickets in
any column.

**But see 027.** If search covers ticket bodies across boards, the notes are
findable where they already are and the export may not be worth building at
all. Do the template section now; decide the export after search exists.

Where it would live: `tasks/engineering/`, under the board so `AMD_DIR` moves
the whole thing and a board stays self-contained. Committed, not gitignored —
being findable is the point.

Framing that helps: this is ADR's little sibling. ADRs record decisions, these
record findings. Same durability, far less ceremony, one file per finding
rather than one per ticket.

## Acceptance criteria

- [ ] `## Engineering notes` in the built-in ticket template, with `###` per finding
- [ ] The board README explains what the section is for
- [ ] Decide, after 027 lands, whether the export is still worth building
- [ ] If it is: `amd notes export` is idempotent and regenerates from scratch
- [ ] If it is: covers every column, not just `done/`
