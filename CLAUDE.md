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
  overrides, `TaskContext`.
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
- Env vars: `AMD_DIR` (board dir name, default `tasks`), `AMD_YES` (force-create),
  `EDITOR` (for `amd edit`).

## Templates (the reason this is Rust)

- Built-ins are compiled in via `include_str!` (`task`, `bug`, `board-readme`).
  Anything in `<board>/templates/<name>.md.jinja` overrides or adds to them;
  `amd templates eject <name>` writes an editable copy there.
- The environment is deliberately configured: `UndefinedBehavior::Strict` (a
  typo'd variable is an error, not a blank line), auto-escape **off** (output is
  markdown, not HTML), `keep_trailing_newline`, `trim_blocks`, `lstrip_blocks`.
- Custom `yaml` filter quotes/escapes frontmatter values, so a title containing
  `"` can't corrupt the block. Use it for every frontmatter value.
- `TaskContext` is the contract with templates (`id`, `number`, `title`, `slug`,
  `tags`, `created`, `timestamp`, `column`, `author`, `email`, `board`,
  `template`, `extra`). Adding a field is additive; renaming one breaks every
  user template — treat it as a public API.
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
  templates; install downloads a prebuilt binary instead of copying a script.
  Same CLI, same board layout, same file format — v3 boards work unchanged.

## Conventions

- Public, unprotected repo — push docs/fixes to `main` directly; workflow-file
  changes need a `workflow`-scoped token.
- Co-authored commits use the bot identity, not the Anthropic no-reply:
  `Co-Authored-By: Claude <309050497+MrDClaudeBot@users.noreply.github.com>`.
