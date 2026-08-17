# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Scope

`agile-md` is a tiny **filesystem Kanban**. The whole tool is the single bash
script `amd`: it manages markdown task files moved between `backlog/`, `todo/`,
`doing/` and `done/` directories. Board data lives in each *consuming* repo at
`<repo-root>/tasks/`; the `amd` command is installed globally and operates on
whatever repo you're in. Pure `bash` + `git` — no JavaScript, no runtime, no
dependencies. Design principles: **status is the folder**, **git history is the
audit trail** (moves are `git mv`), tasks are self-describing markdown.

## Layout

- `amd` — the CLI (the entire tool).
- `install.sh` — installs `amd` to `/usr/local/bin` if writable, else `~/bin`
  (`--dir` overrides). Works both from a clone (copies `./amd`) and via
  `curl … | bash` (fetches `raw…/vN/amd`) — keep those two paths in sync.
  `--with-skill` installs `.claude/skills/agile-md/SKILL.md` to
  `~/.claude/skills/`, by the same two paths; opt-in, because installing the
  tool shouldn't write into another tool's config. The curl path only works
  once the moving major tag points at a commit containing the file.
- `.claude/skills/` — skills committed to the repo. `shell-scripting` and
  `python-scripting` are about *building* the tool; `agile-md` is about *using*
  a board, and is the one that ships to users.
- `tests/test.sh` — assert-based suite; spins up temp git repos.
- `.github/workflows/ci.yml` — shellcheck + tests.
- `README.md`, `LICENSE` (MIT).

## How `amd` works (architecture)

- `board_dir()` resolves the board as `$(git rev-parse --show-toplevel)/${AMD_DIR:-tasks}`,
  so `amd` works from any subdirectory; it errors if not inside a git repo.
- Tasks are `NNN-slug.md`, zero-padded — a stable id and a default ordering.
  `next_id()` = **max of the `.next-id` counter and (highest id on disk + 1)**.
  The counter is what remembers an id whose task was deleted outright rather
  than archived; the max-with-disk fallback is what stops a lost, stale or
  badly merged counter from reissuing an id that is still in use. Losing
  `.next-id` costs only the deleted ids. `record_id` advances it after `new`
  and only ever forwards; `ensure_counter` seeds it from the high-water mark on
  boards that predate it, from `ensure_board` — so adoption happens on the first
  command of any kind, not the first `new`, closing the window where a task
  deleted in between would have its id reissued. No migration script is needed
  or possible: `install.sh` installs a global command and cannot know which
  repos have boards. Arithmetic on a padded id needs `$((10#${id}))` —
  `$((008))` is an octal error — but `highest_id`'s sed already strips the
  padding, and `10#` does not work inside `[ ]` at all.
- `find_task <ref>` resolves a numeric id or a unique slug substring to one file
  (errors on 0 or >1 matches).
- Frontmatter carries `repository` and `priority`. `list_column` builds
  `rank<TAB>id<TAB>line` rows so one `sort` serves both orderings (`-s
  id|priority`), then `cut`s the keys back off; `repo_matches` filters on a
  case-insensitive substring of `repository`. Unset/unknown priorities rank
  last, so boards predating the fields still list and sort — don't "fix" that
  by defaulting them to medium at read time.
- `set_field` rewrites one frontmatter key, inserting it before the closing
  `---` when the task predates it. Anything that can `die` must be assigned to
  a variable first (`p="$(priority_of "$x")"`) — a `die` inside a substitution
  used directly as an argument only kills the subshell, and the caller happily
  proceeds with an empty string.
- `move()` uses `git mv` for tracked files (history follows the rename), else
  plain `mv`. It does **not** commit — the user commits the move.
- `COLUMNS` is ordered left to right and is the single source of column truth:
  `new` writes into the first (`backlog`), `back` walks it in reverse, and
  `next_id`/`find_task`/`list_column` iterate it. Adding a column means editing
  that array, `back`'s case, `parse_filters`'s accepted list, and adding a move
  command named for it. `ensure_columns` creates any missing column on every
  command, so a board written by an older amd gains `backlog/` on first use
  instead of `new` failing on a directory that isn't there.
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
- Env vars: `AMD_DIR` (board dir name, default `tasks`), `AMD_YES` (force-create),
  `AMD_SORT` (default ordering, `id` or `priority`), `MARKDOWN_EDITOR`/`EDITOR`
  (`amd edit`).
- `resolve_editor` picks the first of `MARKDOWN_EDITOR`, `EDITOR`, `vi` that is
  non-empty *and* on PATH (`command -v` on the first word), and puts it in the
  `EDITOR_ARGV` array so values with arguments (`code --wait`) still work — the
  old `"${EDITOR:-vi}" "$file"` couldn't. Returning it as a string would undo
  that, so it stays a global array.

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

## Status

The bash implementation is **no longer maintained**; `v5.0.0` is its final
release. The tags stay published and existing installs keep working, so don't
break the `v5` major tag or the installer — but new work goes to the Rust
implementation on `rust/gui-board`, which has the desktop board, epics and
sprints, points and ordering.

Boards are compatible: same columns, same ticket files. The CLIs are not —
`amd doing` there is `amd start`, and `amd set` takes different fields — so
don't copy invocations between them.

## Versioning & releasing (no release workflow — do it by hand)

Cut releases manually: `git tag -a vX.Y.Z`, force-move the major tag
(`git tag -f -a vN`), push both, `gh release create`. When the **major** bumps,
update the `RAW=.../agile-md/vN` line in `install.sh` and the `curl` URL in
`README.md`. Invocation model by major (breaking changes):

- **v1** — script vendored per board (`tasks/task`); board = the script's own dir.
- **v2** — global `task` command; board discovered from the repo root; `task init`.
- **v3** — command renamed `task` → **`amd`**; env `TASK_YES`/`TASKS_DIR` →
  `AMD_YES`/`AMD_DIR`. `@v3`/`@v2` are moving major tags.
- **v4** — `amd start` renamed to **`amd doing`**, so every move command is
  named for the column it moves the task into (`doing`, `done`). No alias is
  kept: `amd start` is an unknown command.
- **v5** — `backlog/` added as the first column and `amd new` creates there
  rather than in `todo/`; `amd todo <ref>` promotes. Existing boards gain the
  directory automatically, but anything scripted against `tasks/todo/NNN-*.md`
  right after `amd new` breaks. **The last release of the bash line.**

## Conventions

- Public repo; `main` is **protected** — everything lands via a pull request,
  including one-line docs and CI fixes. Workflow-file changes additionally need
  a `workflow`-scoped token to push.
- Co-authored commits use the bot identity, not the Anthropic no-reply:
  `Co-Authored-By: Claude <309050497+MrDClaudeBot@users.noreply.github.com>`.
