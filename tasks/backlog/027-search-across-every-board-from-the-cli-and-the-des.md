---
id: "027"
title: "Search across every board, from the CLI and the desktop"
assignee: ""
branch-type: "feature"
branch-name: "feature/search-across-every-board-from-the-cli-and-the-des"
parent: ""
related: [026]
tags: [search]
created: "2026-08-20"
---

## Notes

`grep -r` already works in one repo. The value this adds is the parts grep
cannot do:

- **Across every registered board.** The registry knows the repositories; you
  do not have to know the paths. This is the main reason to build it.
- **In the desktop board**, as a filter box over the cards — probably the
  higher-value half, since the GUI is the front end most people will use.
- **Board-aware**: skips `archive/`, knows about columns and groups, and
  returns ticket ids you can then act on.

### Fuzzy where fuzzy helps, and not where it hurts

Fuzzy (subsequence) matching is right for **short strings** — ids, slugs,
titles — and it is what makes `amd show chec` find `checkout-revamp`.

It is actively bad for **prose**. Fuzzy-matching a paragraph matches almost
everything and ranks noise above signal. Bodies want substring or word
matching, ranked by hit count and by where the hit landed (a title hit beats a
body hit).

Conflating the two gives a search that feels broken. Keep them separate.

### Where it lives

`Board::search` belongs in **amd-lib** — both front ends need the same results
in the same order, and ranking is part of the result, not presentation
(ADR-0006). The matcher crate becomes an amd-lib dependency, which is fine: it
is not a UI dependency.

No index. The markdown changes under you on every checkout and pull, so an
index is stale the moment it is written — the same reason the registry stores
paths and nothing else. Search reads from disk. The desktop board already holds
loaded cards, so a filter box can work over those and only re-read on Reload.

### Interaction with 026

If this covers ticket bodies, engineering notes become findable where they
already are, and the export half of 026 may not be worth building. Land this
first.

## Acceptance criteria

- [ ] `amd search <query>` searches every registered board, not just the current one
- [ ] Output names the repository when more than one is in view
- [ ] `--json` for agents (ADR-0006)
- [ ] Fuzzy on titles/ids/slugs; substring on bodies; title hits rank above body hits
- [ ] A filter box on the desktop board
- [ ] The archive is not searched
- [ ] No index is written anywhere
