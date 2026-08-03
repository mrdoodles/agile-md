# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Scope

`agile-md` is a tiny **filesystem Kanban**. The whole tool is the `amd` binary:
it manages markdown task files moved between `todo/`, `doing/` and `done/`
directories. Board data lives in each *consuming* repo at `<repo-root>/tasks/`;
`amd` is installed globally and operates on whatever repo you're in. Rust +
`git`, shipped as a single static binary — no runtime, no daemon, no database.
Design principles: **status is the folder**, **git history is the audit trail**
(moves are `git mv`), tasks are self-describing markdown **rendered from
MiniJinja templates**.

## Layout

- `src/main.rs` — clap CLI and command handlers.
- `src/board.rs` — board discovery (`<repo-root>/${AMD_DIR:-tasks}`), the
  `Column` enum, task lookup, moves, `ensure`/`create`.
- `src/task.rs` — `Task` (path, column, id, stem), frontmatter `meta` lookup,
  `slugify`.
- `src/templates.rs` — the MiniJinja `Environment`, built-in templates, board
  overrides, `TaskContext`, `required_extras`.
- `src/form.rs` — every interactive prompt, built on `inquire` (text, select,
  confirm, task picker, label autocomplete).
- `src/branch.rs` — the label taxonomy: conventional-commit types, the
  type -> branch-prefix map, and git ref-name validation.
- `src/render.rs` — board output: rich tables (`richrs`) on a terminal, plain
  greppable text everywhere else.
- `src/git.rs` — thin wrappers over the `git` CLI (`rev-parse`, `ls-files`,
  `mv`, `config`).
- `templates/*.md.jinja` — the built-in templates, `include_str!`'d into the
  binary.
- `install.sh` — downloads the prebuilt `amd-<target>.zip` from the GitHub
  release, or builds from source in a clone (`--from-source`, `--dir`,
  `--version`). Keep the two paths in sync.
- `tests/test.sh` — assert-based end-to-end suite; builds with cargo and spins
  up temp git repos. Unit tests live next to the code in `#[cfg(test)] mod tests`.
- `.github/workflows/ci.yml` — fmt + clippy + cargo test + tests/test.sh, and
  shellcheck for the shell files.
- `.github/workflows/release.yml` — on a `vX.Y.Z` tag, calls
  `mrdoodles/rust-release` to build and attach `amd-<target>.zip` per platform.
- `README.md`, `LICENSE` (MIT).

## How `amd` works (architecture)

- `Board::locate()` resolves the board as `$(git rev-parse --show-toplevel)/${AMD_DIR:-tasks}`,
  so `amd` works from any subdirectory; it errors if not inside a git repo.
- Tasks are `NNN-slug.md`. `Board::next_id()` = max existing id across all
  columns + 1, zero-padded — a stable id and a default ordering.
- `Board::find(<ref>)` resolves a numeric id or a unique slug substring to one
  task (errors on 0 or >1 matches).
- `Board::move_task()` uses `git mv` for tracked files (history follows the
  rename), else `fs::rename`. It does **not** commit — the user commits the move.
- `Board::ensure()` runs before any board-requiring command: if there's no
  board, it prompts to create one when interactive (`stdin().is_terminal()`),
  auto-creates when `AMD_YES=1`, and otherwise errors (never hangs
  non-interactively).
- Env vars: `AMD_DIR` (board dir name, default `tasks`), `AMD_TYPES` (type
  labels), `AGILE_MD_SCOPES` (extra scope labels), `AMD_YES` (force-create), `AMD_NO_INPUT` (never prompt),
  `AMD_NO_BRANCH` (never touch branches), `NO_COLOR`/`--plain` (plain board),
  `EDITOR` (for `amd edit`).

## Labels and branches (`src/branch.rs`)

- **One ticket kind, four labels**: `type` and `scope` (required), `epic` and
  `story` (optional groupings, indexed by `amd epics` / `amd stories`), plus
  free tags.
- **`scope` decides whether there is a branch; `type` decides its name.**
  `code` (the default) branches, everything else doesn't — an admin ticket has
  nothing to check out, so it gets an empty `branch:` and `amd start` says why.
  `AGILE_MD_SCOPES` *adds* to `code`/`admin` (note: `AMD_TYPES` *replaces*, and
  this variable keeps the spelled-out prefix the ticket asked for).
- Scope is settled before the title in the form, because the title's validation
  depends on it: branch rules for code scope, "must make a filename" otherwise
  (`branch::validate_sluggable`).
- A task with no `scope` at all (created before scopes existed) keeps the old
  behaviour and still branches.
- `type` values are **conventional-commit** types (`feat`, `fix`, `docs`, …);
  branches use the **branch** convention (`feature/`, `bugfix/`, `hotfix/`,
  `chore/`). `prefix()` maps between them, matching the two lists in
  mrdoodles/conventional-validator — so commits *and* branches validate.
  `AMD_TYPES` overrides the accepted types; an entry that is already a branch
  prefix keeps its own name.
- `amd start` moves the task and then switches to its branch. **Order matters**:
  `git mv` is staged first so the rename travels with the switch and lands on
  the task's branch. The branch is read from the ticket's `branch:` frontmatter
  (written at creation, so it's editable), then `--branch`, then type + title.
- Titles are validated against git's ref rules at creation — the title becomes
  the branch, so a title with nothing sluggable in it is rejected up front
  rather than yielding a ticket that can never start. `slugify` returns an
  empty string in that case; callers must reject it.

## Board rendering (`src/render.rs`)

- Rich tables only when stdout is a terminal and neither `--plain` nor
  `NO_COLOR` is set; otherwise the plain `  [id] title  (labels)` form, which
  is what `tests/test.sh` asserts on and what pipes into `grep`.
- `richrs` sizes its borders from header widths, so setting `Column::min_width`
  produces borders that don't match the rows. Cells are therefore padded and
  ellipsised here (`fit()`) before being handed over — which also lines the
  three column tables up with each other.
- A rendering failure falls back to plain output rather than erroring: never
  let cosmetics stop someone seeing the board.

## Forms (`src/form.rs`)

- **Prompting is always optional.** `form::available()` gates every prompt on
  `stdin` *and* `stdout` being a terminal, plus `--no-input`/`AMD_NO_INPUT=1`.
  Non-interactively a missing value is an error with the usage line — the same
  contract the bash version had, and why the tool never hangs in CI.
- Optional arguments drive it: `amd new` with no title runs the full form
  (template, type, title, epic, story, tags), `-i` re-asks for what was passed, and
  `start`/`done`/`back`/`show`/`edit` with no `<ref>` open a task picker
  scoped to the columns that command can act on.
- **The form is derived from the template**: `templates::required_extras()`
  scans the source for `extra.<key>` / `extra["key"]`, and `cmd_new` prompts
  for each one that `--set` didn't supply. Adding a field to a template adds a
  question — there is deliberately no separate field registry to keep in sync.
- `tests/test.sh` exports `AMD_NO_INPUT=1` so the suite stays hermetic when run
  from a terminal; it covers the non-interactive halves of these paths. Drive
  the interactive halves by hand through a pty (`script -q /dev/null amd new`).
- Esc/Ctrl-C map to a plain `cancelled` error, not a panic or a partial task.
- **The form ends in the body.** `form::body()` renders the ticket into a temp
  file, opens `$EDITOR` on it (splitting the command so `code --wait` works)
  and returns what came back; `cmd_new` only writes to the board afterwards.
  A non-zero editor exit or an emptied file creates nothing. Deliberately not
  inquire's `Editor` prompt: that waits for an `e` keypress first, which is the
  stop this was meant to remove. Default on when the form ran or `-e`; off with
  `--no-edit` and whenever prompting isn't available, so scripts and CI never
  spawn an editor. Unit-testable without a TTY — pass `true`/`false` as the
  editor command.

## Templates (the reason this is Rust)

- Built-ins are compiled in via `include_str!` (`task`, `board-readme`).
  Anything in `<board>/templates/<name>.md.jinja` overrides or adds to them;
  `amd templates eject <name>` writes an editable copy there.
- The environment is deliberately configured: `UndefinedBehavior::Strict` (a
  typo'd variable is an error, not a blank line), auto-escape **off** (output is
  markdown, not HTML), `keep_trailing_newline`, `trim_blocks`, `lstrip_blocks`.
- Custom `yaml` filter quotes/escapes frontmatter values, so a title containing
  `"` can't corrupt the block. Use it for every frontmatter value.
- `TaskContext` is the contract with templates (`id`, `number`, `title`, `slug`,
  `type`, `scope`, `epic`, `story`, `branch`, `tags`, `created`, `timestamp`, `column`,
  `author`, `email`, `board`, `template`, `extra`). Adding a field is additive;
  renaming one breaks every user template — treat it as a public API.
- Template errors are flattened by `render_error()` (message + line + cause
  chain + MiniJinja debug info); without that only the top line survives.

## Commands

```bash
cargo test
bash tests/test.sh
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all --check
bash install.sh [--dir DIR] [--version vX.Y.Z] [--from-source]
```

## Coding style

- Edition 2024, `rust-toolchain.toml` pins **stable**, MSRV 1.88 (let-chains).
  CI gates on `cargo fmt --check` + `cargo clippy -D warnings` + tests.
- Errors: `anyhow` with `.with_context()`; `main` prints `amd: {err:#}` and
  exits 1, matching the old bash `die()`.
- Shell out to `git` rather than linking a git library — behaviour is exactly
  what the user would get typing the command.
- No `unwrap()`/`expect()` on anything user input can reach.
- Keep the CLI surface stable: `tests/test.sh` is the spec, and the assertions
  there are the compatibility contract with v3 boards.
- The shell files still follow the repo's bash rules (see the `shell-scripting`
  skill): quote the literal `"done"` (SC1010), `set -euo pipefail`, and
  shellcheck `--severity=warning` clean.

## Versioning & releasing

Cut releases by tag: `git tag -a vX.Y.Z && git push --tags` triggers
`release.yml`, which builds and attaches `amd-<target>.zip`. Force-move the
major tag (`git tag -f -a vN`) and push it, since `install.sh` and the README
`curl` URL point at `raw…/agile-md/vN`. Invocation model by major (breaking
changes):

- **v1** — script vendored per board (`tasks/task`); board = the script's own dir.
- **v2** — global `task` command; board discovered from the repo root; `task init`.
- **v3** — command renamed `task` → **`amd`**; env `TASK_YES`/`TASKS_DIR` →
  `AMD_YES`/`AMD_DIR`.
- **v4** — bash script → **Rust binary**; tasks rendered from MiniJinja
  templates; `inquire` forms when a value is missing; `type`/`epic`/`story`
  labels with branch creation on `amd start`; rich board output. Install
  downloads a prebuilt binary instead of copying a script. The v3 CLI and board
  layout still work; v3 task files simply have no labels, so `amd start` on one
  leaves the branch alone.

## Conventions

- Public, unprotected repo — push docs/fixes to `main` directly; workflow-file
  changes need a `workflow`-scoped token.
- Co-authored commits use the bot identity, not the Anthropic no-reply:
  `Co-Authored-By: Claude <309050497+MrDClaudeBot@users.noreply.github.com>`.
