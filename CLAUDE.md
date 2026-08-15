# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Scope

`agile-md` is a tiny **filesystem Kanban**. The whole tool is the single bash
script `amd`: it manages markdown task files moved between `todo/`, `doing/` and
`done/` directories. Board data lives in each *consuming* repo at
`<repo-root>/tasks/`; the `amd` command is installed globally and operates on
whatever repo you're in. Pure `bash` + `git` — no JavaScript, no runtime, no
dependencies. Design principles: **status is the folder**, **git history is the
audit trail** (moves are `git mv`), tasks are self-describing markdown.

## Layout

- `amd` — the CLI (the entire tool).
- `install.sh` — installs `amd` to `/usr/local/bin` if writable, else `~/bin`
  (`--dir` overrides). Works both from a clone (copies `./amd`) and via
  `curl … | bash` (fetches `raw…/v3/amd`) — keep those two paths in sync.
- `tests/test.sh` — assert-based suite; spins up temp git repos.
- `.github/workflows/ci.yml` — shellcheck + tests.
- `README.md`, `LICENSE` (MIT).

## How `amd` works (architecture)

- `board_dir()` resolves the board as `$(git rev-parse --show-toplevel)/${AMD_DIR:-tasks}`,
  so `amd` works from any subdirectory; it errors if not inside a git repo.
- Tasks are `NNN-slug.md`. `next_id()` = max existing id across all columns + 1,
  zero-padded — a stable id and a default ordering.
- `find_task <ref>` resolves a numeric id or a unique slug substring to one file
  (errors on 0 or >1 matches).
- `move()` uses `git mv` for tracked files (history follows the rename), else
  plain `mv`. It does **not** commit — the user commits the move.
- `archive/` is the drawer: `ensure_archive` creates it *with* its `.gitignore`
  (`*` + `!.gitignore`) and is called from `create_board`, `archive_task` and
  `ensure_board`, so the directory can never exist unprotected — that is how
  archived tasks would silently get committed. `archive_task` does
  `git rm --cached` then a plain `mv` (a `git mv` into an ignored path is
  refused, and forcing it defeats the `.gitignore`). `next_id` counts archived
  ids so they're never reused; `find_task` does *not* search archive, so an
  archived task stops resolving as a `<ref>`.
- `ensure_board()` runs before any board-requiring command: if there's no board,
  it prompts to create one when interactive (`[ -t 0 ]`), auto-creates when
  `AMD_YES=1`, and otherwise errors (never hangs non-interactively).
- Env vars: `AMD_DIR` (board dir name, default `tasks`), `AMD_YES` (force-create).

## Commands

```bash
bash tests/test.sh
shellcheck -x --severity=warning amd install.sh tests/*.sh
bash install.sh [--dir DIR]
```

## Coding style — subtle bash rules this repo depends on

- Pure `bash` with `set -euo pipefail`; must pass `shellcheck -x --severity=warning`
  (CI enforces). That severity is deliberate: it fails on real warnings but not
  on info/style noise (e.g. dynamic `source` paths).
- **`done` is a shell keyword.** Whenever it appears as a literal string (the
  `COLUMNS` array, a case value, a command argument), quote it as `"done"` or
  shellcheck SC1010 fails.
- **`set -e` + function return values**: a function whose *last* statement is
  `[ cond ] && cmd` returns non-zero when `cond` is false, which aborts the
  caller under `set -e`. End functions with `if [ cond ]; then cmd; fi` instead
  (see `list_column` — this bug once made the DONE column silently not print).
- Prefer `awk`/`printf` over `sed … | head` pipelines that can SIGPIPE under
  `pipefail` (see `meta()`).
- Keep it dependency-free and self-contained — the script is copied verbatim onto
  users' PATH.
- The word "task" still means a *work item* throughout (comments, `find_task`);
  only the command is named `amd`.

## Versioning & releasing (no release workflow — do it by hand)

Cut releases manually: `git tag -a vX.Y.Z`, force-move the major tag
(`git tag -f -a vN`), push both, `gh release create`. When the **major** bumps,
update the `RAW=.../agile-md/vN` line in `install.sh` and the `curl` URL in
`README.md`. Invocation model by major (breaking changes):

- **v1** — script vendored per board (`tasks/task`); board = the script's own dir.
- **v2** — global `task` command; board discovered from the repo root; `task init`.
- **v3** — command renamed `task` → **`amd`**; env `TASK_YES`/`TASKS_DIR` →
  `AMD_YES`/`AMD_DIR`. `@v3`/`@v2` are moving major tags.

## Conventions

- Public, unprotected repo — push docs/fixes to `main` directly; workflow-file
  changes need a `workflow`-scoped token.
- Co-authored commits use the bot identity, not the Anthropic no-reply:
  `Co-Authored-By: Claude <309050497+MrDClaudeBot@users.noreply.github.com>`.
