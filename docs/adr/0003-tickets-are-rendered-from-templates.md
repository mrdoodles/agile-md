# ADR-0003: Tickets are rendered from templates

- **Status:** Accepted
- **Date:** 2026-08-18

## Context

Every board wants slightly different things on a ticket. The usual answers are
a fixed format nobody can change, or a configuration language that slowly
becomes a bad programming language.

Ticket files also have to stay valid: a title containing a quote must not
corrupt the frontmatter, and a typo in a template must not silently produce a
ticket with a blank field.

## Decision

**Ticket files are rendered from MiniJinja templates.** The built-ins are
compiled into the binary with `include_str!`; anything in
`<board>/templates/<name>.md.jinja` overrides or adds to them, and
`amd templates eject <name>` writes an editable copy.

The environment is configured deliberately:

- `UndefinedBehavior::Strict` — a typo'd variable is an error, not a blank line.
- Auto-escape **off** — the output is markdown, not HTML.
- `keep_trailing_newline`, `trim_blocks`, `lstrip_blocks`.
- A custom `yaml` filter quotes and escapes frontmatter values. Every
  frontmatter value goes through it.

**The template is the form.** `templates::required_extras()` scans the source
for `extra.<key>` / `extra["key"]`, and the CLI prompts for each one the
caller did not supply.

## Consequences

- Adding a field to a template adds a question to the form. There is
  deliberately no separate field registry to keep in step.
- A board's ticket format is the board's business, and it travels in the
  repository with the tickets it produces.
- **`TaskContext` is a public API.** Adding a field is additive; renaming one
  breaks every user template on every board. Treat it with the care of a
  published interface.
- Template errors are flattened by `render_error()` — message, line, cause
  chain, MiniJinja debug info — because without that only the top line
  survives, and "syntax error" with no location is not a bug report.
- **Cost:** a dependency on a template engine, and users can write templates
  that produce nonsense. Strict undefined and the `yaml` filter cover the
  failures that would corrupt a board.

## Alternatives considered

- **A fixed ticket format.** Rejected: every team wants one more field, and
  the tool becomes a queue of feature requests for fields.
- **A config file listing fields.** Rejected: it is a template with fewer
  capabilities, plus a registry to keep in sync with the renderer — the exact
  drift this design avoids.
