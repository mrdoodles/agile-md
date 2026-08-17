---
id: "025"
title: "Overlay errors should show in the overlay"
assignee: ""
branch-type: ""
branch-name: ""
parent: ""
related: []
tags: []
created: "2026-08-17"
points: "2"
---

## Notes

When saving from an overlay fails, the message goes to the status line in the
top panel — which is *behind* the modal. The overlay stays open with no
explanation, so a failed save is indistinguishable from a button that does
nothing.

Found while fixing the new-ticket overlay: `Draft::write` was handed a body
with no frontmatter, `set_points` then failed with "no frontmatter to update",
and the only symptom anyone could see was that Create appeared inert. The
underlying bug is fixed, but any future save error will present exactly the
same way.

Affects all three overlays — new ticket, edit ticket, and the epic/sprint
editor — and the sprint rules make it likely rather than theoretical: filing
an unsized ticket into a sprint, or touching a started one, both fail by
design and both currently fail silently from the user's point of view.

## Acceptance criteria

- [ ] A failed save shows its error inside the overlay, next to the buttons
- [ ] The overlay stays open on failure so the typing is not lost
- [ ] A successful save clears any previous error
- [ ] Covers the new ticket, edit ticket and epic/sprint overlays
- [ ] The status line keeps reporting errors that happen outside an overlay,
      such as a refused drag
