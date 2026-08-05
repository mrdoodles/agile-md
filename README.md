# agile-md

A tiny **filesystem Kanban** — tasks are markdown files moved between `todo/`,
`doing/` and `done/` directories. A single static binary plus `git`; no runtime,
no daemon, no database.

- **Status is the folder.** A task's column is simply which directory it's in.
- **Git is the audit trail.** Moving a task is a `git mv`, so `git log --follow`
  reconstructs exactly when it started and finished.
- **Tickets come from templates.** Task files are rendered from
  [MiniJinja](https://github.com/mitsuhiko/minijinja) templates, so the format
  is consistent by construction and yours to change.
- **The template is the form.** Leave an argument out in a terminal and `amd`
  asks — including the fields your own template declares.
- **One type per ticket.** `admin`, or the conventional-commit type of the
  change it makes. Development work is tracked on a branch named from that type
  — `amd start` puts you on `feature/add-login` — and admin work gets none.
- **One command, any repo.** Install `amd` once on your PATH; it finds the
  board at `<repository-root>/tasks` from wherever you are.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/mrdoodles/agile-md/v4/install.sh | bash
```

Downloads the prebuilt binary for your platform (Linux/macOS, x86_64/aarch64)
into `/usr/local/bin` when it's writable, otherwise `~/bin`. Override with
`install.sh --dir <dir>`, pin with `--version vX.Y.Z`, or build locally with
`--from-source` (needs [Rust](https://rustup.rs)). From a clone:

```bash
cargo build --release        # target/release/amd
./install.sh                 # builds and installs
```

## Use

In any git repository:

```bash
amd init                          # scaffold tasks/{todo,doing,done} here
amd new "Publish to Marketplace" --type feat --epic launch
amd board                         # show all columns (the default)
amd start 1                       # todo -> doing, and switch to feature/publish-to-marketplace
amd done  1                       # doing -> done
amd back  1                       # move one column left
amd show  publish                 # print a task (id or slug substring)
amd edit  1                       # open in $EDITOR
amd link  1 3                     # relate two tickets
```

`amd` works from any subdirectory — it resolves the board from the repo root.
Set `AMD_DIR` to use a board directory other than `tasks`.

## Ticket types

A ticket's `type` is one field with one list: **`admin`**, or the
conventional-commit type of the change it makes.

| type | branch | frontmatter |
| --- | --- | --- |
| `admin` | none — a rota or a renewal has nothing to check out | no `type`, no `branch` |
| `feat` `fix` `docs` `style` `refactor` `perf` `test` `build` `ci` `chore` `revert` | `<prefix>/<title>` | `type`, `branch` |

```bash
amd new "Add login"                        # feat, the default
amd new "Renew the certificates" --type admin
amd start 1                                # development: switches to feature/add-login
amd start 2                                # admin: "admin tickets don't use branches"
```

Which template renders a ticket follows from the type — `admin` gets the admin
template, everything else the development one — so there is nothing else to
choose. `--template` still overrides it for a board with templates of its own,
and the rule for whether a branch appears lives in the template: one that
records a `branch` gets one.

## Labels and nesting

On top of the ticket type:

| Field    | On          | Values                                                     |
| -------- | ----------- | ---------------------------------------------------------- |
| `type`   | development | conventional-commit types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert` (default `feat`; the list is `AMD_TYPES`) |
| `parent` | both        | the ticket this one sits under — optional, and any depth     |
| `assignee` | both      | who it belongs to — optional, `@me` resolves to your git name |
| `tags`   | both        | free-form, completed against the tags already on the board   |

**One `parent` instead of epic and story fields.** Nesting gives you both, and
anything past them, without deciding up front which level a ticket is:

```bash
amd new "Checkout revamp"
amd new "Guest checkout" --parent 1
amd new "Address form"   --parent 2
```

```
TODO  (4)
├── [001] Checkout revamp  (feat)
│   └── [002] Guest checkout  (feat)
│       └── [003] Address form  (feat)
└── [004] Unrelated chore  (chore)
```

The link is navigable from both ends: the child gets a `## Parent` wikilink, the
parent a `## Children` list, and on a terminal that supports hyperlinks the ids
on the board open the ticket when you click them.

```markdown
## Parent

[[001-checkout-revamp]]
```

Children are nested inside their column — a child whose parent is in another
column stays at the top level with a `^001` marker, since the columns are what
the board is for. Everything is ordered by id at every level.

The change type **names** the branch. Ticket types are *commit* types, branches
follow the *branch* convention, and `amd` maps between them — so both halves
satisfy [conventional-validator](https://github.com/mrdoodles/conventional-validator):

| change type                                     | branch prefix |
| ----------------------------------------------- | ------------- |
| `feat`                                          | `feature/`    |
| `fix`                                           | `bugfix/`     |
| `revert`                                        | `hotfix/`     |
| `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore` | `chore/`      |

```bash
amd new "Crash on save" --type fix     # created todo/003-crash-on-save.md (bugfix/crash-on-save)
amd start 3 --branch spike/try-it      # …or a branch you name
amd start 3 --no-branch                # …or none at all (AMD_NO_BRANCH=1 for good)
```

The branch name is written into the ticket at creation, so you can see it — and
edit it — before any work starts. Because the title becomes a branch, it's
**validated when you type it**: a title with nothing sluggable in it is rejected
rather than producing a ticket that can never start.

## Related tickets

Tickets that depend on each other carry a `related` list — empty by default,
holding ids so a link survives a rename or a move between columns:

```bash
amd new "Session store" --related 1 --related add-logout   # ids or slugs
amd link 3 7                                               # link two that exist
amd link 3 7 --one-way                                     # …or just one end
```

The relation is symmetric: linking 3 to 7 records 7 on ticket 3 *and* 3 on
ticket 7, so it reads the same from either end and you can't forget the other
half. Refs are resolved when they're recorded, so a typo is an error rather
than a dangling link found later.

```markdown
related: [001,002]
```

Prose can still point at tickets with `[[NNN-slug]]` wikilinks; `related` is for
the dependency itself.

## Several repositories at once

Register the repositories you work in, and the TUI shows their boards together:

```bash
amd repos add                 # the one you're standing in
amd repos add ../other-repo
amd repos                     # what's registered
amd repos remove other-repo   # by name or by path
```

In the TUI, `p` picks a repository (or all of them) and `a` picks an assignee
(or everyone, or unassigned). With more than one board in view each card says
which repository it came from, since ids restart at 001 in every repo.

The registry is a **list of repositories, and nothing else** — one path per line
in `~/.config/agile-md/repos`:

```
# Repositories agile-md knows about, one path per line.
/Users/you/code/agile-md
/Users/you/code/other-repo
```

The tickets themselves are never copied into a store. They're markdown in each
repo, they change when you check out a branch or pull someone's work, and an
index of them would be wrong the moment it was written. Reading a board is a
directory listing and a few small files.

## Assignees

```bash
amd new "Add login" --assignee alex   # or -a @me for whoever git says you are
amd assign 3 sam                      # assign an existing ticket
amd assign 3                          # unassign it
```

The name lands in the ticket's frontmatter (`assignee: "sam"`) and shows on the
board as `@sam`, so it moves with the file and is visible in the diff.

## The terminal UI

`amd tui` opens the board full-screen in the terminal — the only front end, and
the one that costs nothing to leave open (2 MB resident, no CPU while idle, and
it works over SSH):

```
┌ TODO  (3) ────────────┐┌ DOING  (1) ───────────┐┌ DONE  (0) ────────────┐
│ [001] Checkout revamp ││ [004] Fix the thing   ││ (empty)               │
│   [002] Guest checkout││                       ││                       │
│ [003] Renew certs  ad ││                       ││                       │
└───────────────────────┘└───────────────────────┘└───────────────────────┘
j/k move  h/l column  [ ] shift  enter view  e edit  n new  p repo  a assignee  s settings  q quit
```

| | |
| --- | --- |
| `j`/`k`, `h`/`l` | move within and between columns (arrows work too) |
| `[` `]` | shift the selected ticket a column left or right |
| `enter` | read the ticket; `e` opens it in `$EDITOR` |
| `n` | new ticket — a title and a type, nothing else to fill in |
| `p` | which repository (or all of them) |
| `a` | whose tickets (or everyone's, or unassigned) |
| `s` | settings |
| `r`, `q` | reload, quit |

**The mouse works**: click a ticket to select it, click it again to open it,
scroll to move through a column, and click **settings** in the bottom-right.

**Themes** come from [ratatui-themes](https://crates.io/crates/ratatui-themes)
— Dracula, Nord, Gruvbox, Catppuccin and the rest. The settings dialog previews
each one as you move through the list, `enter` keeps it and `esc` puts the old
one back. Your choice is remembered in `~/.config/agile-md/config`:

```
theme = nord
```

The TUI is a default feature; `--no-default-features` builds the CLI alone,
which is what CI and scripts want.

## Shell completion

```bash
amd completions bash > /usr/local/etc/bash_completion.d/amd
amd completions zsh  > "${fpath[1]}/_amd"          # then: rm -f ~/.zcompdump; compinit
amd completions fish > ~/.config/fish/completions/amd.fish
```

`amd completions` with no argument works the shell out from `$SHELL`; elvish and
powershell are available too. The script goes to stdout and the install hint to
stderr, so redirecting gives you a clean file.

Completion covers values, not just names — `amd new --type <TAB>` offers the
change types this board accepts (including anything you added with `AMD_TYPES`),
`-T <TAB>` the ticket types, `amd ls <TAB>` the columns. Scripts are generated
from the CLI itself, so they can't drift from it.

## The board

In a terminal `amd board` draws with [richrs](https://crates.io/crates/richrs) —
tree guides, a colour per column, counts, and ids that open the ticket when
clicked:

```
TODO  (3)
├── [001] Checkout revamp  (feat)
│   └── [002] Guest checkout  (feat)
└── [004] Update the guide  (docs)
```

Piped or redirected output stays plain text (`    [002] Guest checkout  (feat)`,
indented by depth), so `amd board | grep` keeps working and log files don't fill
with escape sequences. `--plain` or `NO_COLOR=1` forces that everywhere.

## Why a directory instead of one big TODO.md?

A single file gets unwieldy once tasks have real content, and every edit is a
merge-conflict magnet. Separate files keep tasks self-describing, diffable and
independently movable, while the folders give an unambiguous board.

## Development

```bash
cargo test                    # unit tests (slugify, frontmatter, templates)
bash tests/test.sh            # end-to-end CLI spec against throwaway git repos
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all --check
```

`amd` was pure bash through v3; v4 is the same tool in Rust, which is what
buys the templating. The CLI, the board layout and the file format are
unchanged — v3 boards work as-is.
