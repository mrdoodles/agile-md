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

The crate is a **library plus a binary**, so the CLI and the terminal UI share
one implementation of the board.

- `src/lib.rs` — the library: `board`, `branch`, `create`, `git`, `render`,
  `task`, `templates` (and `tui` behind the default feature).
- `src/create.rs` — making a ticket, split into `Draft::prepare` (resolve and
  render, touching nothing) and `Draft::write` (file, child link, backlinks).
  That gap is what lets the CLI put the rendered ticket in `$EDITOR` before
  anything is written, while the TUI writes what it already has.
- `src/tui/` — the terminal UI (feature `tui`, on by default) and its settings.
- `src/main.rs` — clap CLI and command handlers; the binary also owns
  `form.rs`, `value.rs` and `completions.rs`, which are CLI-only.
- `src/board.rs` — board discovery (`<repo-root>/${AMD_DIR:-tasks}`), the
  `Column` enum, task lookup, moves, `ensure`/`create`.
- `src/task.rs` — `Task` (path, column, id, stem), frontmatter `meta` lookup,
  `slugify` (transliterates via `deunicode`, so a title in any script still
  makes a plain-ASCII filename and branch; symbols stay separators so an emoji
  doesn't become a word).
- `src/templates.rs` — the MiniJinja `Environment`, built-in templates, board
  overrides, `TaskContext`, `required_extras`.
- `src/form.rs` — every interactive prompt, built on `inquire` (text, select,
  confirm, task picker, label autocomplete).
- `src/branch.rs` — the label taxonomy: conventional-commit types, the
  type -> branch-prefix map, and git ref-name validation.
- `src/render.rs` — board output: rich tables (`richrs`) on a terminal, plain
  greppable text everywhere else.
- `src/completions.rs` — `amd completions [SHELL]`, generated from the clap
  command with `clap_complete`; `$SHELL` detection and install hints.
- `src/value.rs` — `TypedValueParser`s that advertise their possible values, so
  completions and `--help` list them while validation stays ours.
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
- Tasks are `NNN-slug.md`. `Board::next_id()` takes whichever is higher of the
  counter in `<board>/.next-id` and one past the highest id in the columns. The
  counter is the authority — it remembers ids whose tickets have gone, junked or
  deleted, which no scan can — and the scan is the safety net, so a lost or
  badly merged counter can't hand out an id already in use. `record_id()` runs
  in `Draft::write`, not `prepare`: an abandoned draft leaves no gap. The junk
  drawer isn't scanned; the counter covers it.
- `Board::find(<ref>)` resolves a numeric id or a unique slug substring to one
  task (errors on 0 or >1 matches).
- `Board::move_task()` uses `git mv` for tracked files (history follows the
  rename), else `fs::rename`. It does **not** commit — the user commits the move.
- `Board::junk()` (`amd rm`) moves a ticket to `<board>/junk/`, which is **not**
  a column: it's off the board and its contents are gitignored. That means
  `git mv` would refuse the ignored destination and forcing it would put the
  junk back into the history the `.gitignore` exists to keep out, so a tracked
  ticket is `git rm --cached`'d and then moved. Junked ids are never reused —
  that's the counter's job — because reusing one would silently repoint every
  `parent` and `related` that named it.
- `Board::ensure()` runs before any board-requiring command: if there's no
  board, it prompts to create one when interactive (`stdin().is_terminal()`),
  auto-creates when `AMD_YES=1`, and otherwise errors (never hangs
  non-interactively).
- Env vars: `AMD_DIR` (board dir name, default `tasks`), `AMD_TYPES` (type
  labels), `AMD_YES` (force-create), `AMD_NO_INPUT` (never prompt),
  `AMD_NO_BRANCH` (never touch branches), `NO_COLOR`/`--plain` (plain board),
  `EDITOR` (for `amd edit`).

## Ticket fields (`src/branch.rs`, `templates/ticket.md.jinja`)

- **One ticket, one template.** There are no ticket types any more: `admin` and
  `development` collapsed into fields you either fill in or don't.
- **`branch-type` is what gives a ticket a branch**, and it's empty by default.
  `branch-name` is derived from it and the title at creation and stored, so it
  can be edited before work starts. No branch type means no branch, and
  `amd start` says so and leaves the working tree alone.
- The values are the **branch** convention (`feature`, `bugfix`, `hotfix`,
  `release`, `chore`), matching mrdoodles/conventional-validator's branch check
  directly — there's no longer a commit-type-to-branch-prefix mapping to keep in
  step. `AMD_BRANCH_TYPES` overrides the list.
- `assignee` says who's doing it, empty by default: tickets are created now and
  assigned later. There is deliberately **no second person field** — who created
  a ticket is already in `git log --follow`, so a `reporter` would duplicate
  history the tool is built on. It was briefly called `owner`; `Task::assignee`
  still reads that key.
- **Frontmatter keys are hyphenated, template variables are not** —
  `branch-type: {{ branch_type | yaml }}`. Jinja would read `branch-type` as a
  subtraction, so the variable has to be `branch_type`; the key is just text.
- `Task` reads the new keys and **falls back to the old ones** (`type`,
  `branch`, `owner`), so boards written before this change still list, start and
  filter correctly. There's no migration command; a ticket picks up the new
  shape when it's rewritten.
- Titles are validated against git's ref rules at creation — the title becomes
  the branch, so a title with nothing sluggable in it is rejected up front.
  `slugify` returns an empty string in that case; callers must reject it.
- `amd start` takes the branch from `--branch`, else the ticket's `branch-name`,
  else derives one from `branch-type` and the title. **Order matters**: `git mv`
  is staged first so the rename travels with the switch.

## Board rendering (`src/render.rs`)

- Each column is a **tree**: `forest()` nests tasks by `parent`, ordered by id
  at every level. A child whose parent is in another column stays at the top
  level with a `^NNN` marker — the columns are the board's primary structure.
  `forest()` is a pure function over `(Task, Option<String>)` pairs, so it's
  unit-tested without touching the filesystem; parents are read once per column
  rather than per lookup (each read is a file read).
- Rich (`richrs::Tree`) only when stdout is a terminal and neither `--plain` nor
  `NO_COLOR` is set; otherwise the same tree as indentation, which is what
  `tests/test.sh` asserts on and what pipes into `grep`.
- Ids are **OSC-8 hyperlinks** in the rich path (`TERM=dumb` opts out), so
  clicking one opens the ticket. They are deliberately not in the plain path:
  escape sequences have no business in a pipe. Note this is why the board moved
  off `richrs::Table` — a table measures cell widths and would count the escape
  bytes, misaligning every border. (`richrs` draws the one-shot `amd board`
  printout only; the interactive board is ratatui.)
- A rendering failure falls back to plain output rather than erroring: never
  let cosmetics stop someone seeing the board.

## Forms (`src/form.rs`)

- **Prompting is always optional.** `form::available()` gates every prompt on
  `stdin` *and* `stdout` being a terminal, plus `--no-input`/`AMD_NO_INPUT=1`.
  Non-interactively a missing value is an error with the usage line — the same
  contract the bash version had, and why the tool never hangs in CI.
- Optional arguments drive it: `amd new` with no title runs the full form
  (title, branch type, assignee, parent, related, tags), `-i` re-asks for what was
  passed, and
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

- Built-ins are compiled in via `include_str!` (`ticket`, `board-readme`).
  Anything in `<board>/templates/<name>.md.jinja` overrides or adds to them;
  `amd templates eject <name>` writes an editable copy there.
- The environment is deliberately configured: `UndefinedBehavior::Strict` (a
  typo'd variable is an error, not a blank line), auto-escape **off** (output is
  markdown, not HTML), `keep_trailing_newline`, `trim_blocks`, `lstrip_blocks`.
- Custom `yaml` filter quotes/escapes frontmatter values, so a title containing
  `"` can't corrupt the block. Use it for every frontmatter value.
- `TaskContext` is the contract with templates (`id`, `number`, `title`, `slug`,
  `branch_type`, `branch_name`, `owner`, `parent`, `parent_link`, `related`, `tags`, `created`, `timestamp`, `column`,
  `author`, `email`, `board`, `template`, `extra`). Adding a field is additive;
  renaming one breaks every user template — treat it as a public API.
- Template errors are flattened by `render_error()` (message + line + cause
  chain + MiniJinja debug info); without that only the top line survives.

## Several repositories (`src/registry.rs`)

- The registry is **a list of repository paths and nothing else**, one per line
  at `$XDG_CONFIG_HOME/agile-md/repos`. There is deliberately no store of
  tickets: the markdown is the source of truth, it changes under you on every
  checkout and pull, and an index would be stale the moment it was written.
  Reading a board is a directory listing and a few small files.
- **The list fills itself**: `registry::remember()` runs whenever a command
  resolves a board, writing only the first time a repository is seen, so the
  boards you work on accumulate across sessions. Best effort — a read-only
  config directory must not stop `amd board`. `AMD_NO_REGISTER=1` opts out, and
  only then does `amd repos remove` stick.
- **`amd repos` and `amd completions` run before the board is resolved.** Asking
  what's on the list must not add to it, and both have to work outside a repo.
  That bug shipped for one commit and the CLI suite now covers it.
- `Registry::boards()` always includes the repository you're standing in, listed
  or not — you should never open the TUI and not see the board you're in.
- Ids restart at 001 in every repository, so a `Card` carries which repo it came
  from; the TUI shows the name whenever more than one board is in view, and a
  move goes through *that* repository's `Board`, not the current one.
- `load_from`/`save_to` take a path, so tests need no environment mutation. The
  CLI suite sets `XDG_CONFIG_HOME` to a temp dir — `amd repos add` writes to the
  config directory and a test must never touch the user's.

## The terminal UI (`src/tui/`)

The only front end, and a **default feature** — a build that cannot show you the
board is not much use. `--no-default-features` leaves the CLI for CI and
scripts. Windowed front ends were tried and dropped: a terminal UI costs 2 MB
resident and no idle CPU, works over SSH, and needs no windowing stack. Don't
reintroduce one without a reason that outweighs that.

- It owns no board logic: `Draft` creates, `Board::move_task` moves,
  `render::column_forest`/`flatten` nests. Editing hands over to `$EDITOR`
  around `leave()`/`enter()` rather than reimplementing an editor in a list.
- **Mouse capture** is enabled after `ratatui::init()` and *disabled before*
  `ratatui::restore()` — the other order leaves the shell receiving escape
  sequences. The same pair wraps the `$EDITOR` handoff.
- Clicks resolve against the areas recorded during the last `draw()`
  (`column_areas`, `settings_area`, `dialog_area`), which is why `draw` takes
  `&mut self`. `row_in()` accounts for the widget border.
- **Themes** come from `ratatui-themes`; every colour is a palette lookup, so
  none are hard-coded. The settings dialog previews as you move and restores the
  old theme on `esc`. `settings.rs` persists one line to
  `$XDG_CONFIG_HOME/agile-md/config`, treating anything unreadable as defaults.
  Its `load_from`/`save_to` take a path so the tests need no environment
  mutation — two tests racing on `XDG_CONFIG_HOME` is how that started.
- Testing works through a pty: `script -q /dev/null bash -c "stty rows 40 cols
  130; amd tui"` with timed keystrokes, including SGR mouse sequences. Without
  the `stty` the pty has no size and ratatui draws into a zero-height screen.

## Completions

- Generated, never checked in: `amd completions <shell>` renders from
  `Cli::command()`, so a new flag is completable the moment it exists. Handled
  before `Board::ensure()` — it must work outside a repo, which is exactly when
  people set completion up.
- Value-level completion comes from `src/value.rs`. `TypedValueParser` lets a
  parser advertise `possible_values()` (used by completions and `--help`) while
  `parse_ref()` keeps our own validation and error text — `PossibleValuesParser`
  alone would have replaced "unknown type 'x' … set AMD_TYPES" with clap's
  wording. `TicketType` advertises the built-ins but accepts any name, since a
  board can add templates.
- `clap`'s `string` feature is on for `Str: From<String>`, which the runtime
  value lists need.
- The script goes to stdout, the install hint to stderr, so a redirect gives a
  clean file. `tests/test.sh` runs `bash -n` over the generated bash script.

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
  templates; `inquire` forms when a value is missing; one flexible ticket whose
  `branch-type` decides whether `amd start` makes a branch; a terminal UI.
  Install downloads a prebuilt binary instead of copying a script. The v3 CLI and board
  layout still work; v3 task files simply have no labels, so `amd start` on one
  leaves the branch alone.

## Conventions

- Public, unprotected repo — push docs/fixes to `main` directly; workflow-file
  changes need a `workflow`-scoped token.
- Co-authored commits use the bot identity, not the Anthropic no-reply:
  `Co-Authored-By: Claude <309050497+MrDClaudeBot@users.noreply.github.com>`.
