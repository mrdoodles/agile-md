#!/usr/bin/env bash
#
# End-to-end spec for the amd CLI. Run: bash tests/test.sh
#
# Builds the binary with cargo, then drives it against throwaway git repos.
# Unit tests for the pure logic (slugify, frontmatter, templates) live in the
# Rust sources and run under `cargo test`.
#
# AMD_NO_INPUT=1 keeps the run hermetic: the suite covers the non-interactive
# paths, and without it a prompt would block when the suite is run from a
# terminal.
#
set -uo pipefail

export AMD_NO_INPUT=1

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ -n "${AMD_BIN:-}" ]; then
  AMD="${AMD_BIN}"
else
  echo "building amd..."
  (cd "${ROOT}" && cargo build --quiet) || exit 1
  AMD="${ROOT}/target/debug/amd"
fi
[ -x "${AMD}" ] || { echo "no amd binary at ${AMD}"; exit 1; }

pass=0
fail=0
assert() { # description  command...
  local d="$1"; shift
  if "$@" >/dev/null 2>&1; then echo "  ok   - ${d}"; pass=$((pass + 1))
  else echo "  FAIL - ${d}"; fail=$((fail + 1)); fi
}

tmp="$(mktemp -d)"
nongit="$(mktemp -d)"
trap 'rm -rf "${tmp}" "${nongit}"' EXIT
cd "${tmp}" || exit 1
git init -q; git config user.email t@t.co; git config user.name t

echo "init:"
"${AMD}" init >/dev/null
assert "init creates todo/doing/done" test -d tasks/todo -a -d tasks/doing -a -d tasks/done
assert "init writes the board README" test -f tasks/README.md

echo "discovery from a subdirectory:"
mkdir -p deep/nested
assert "runs from a subdir (finds repo-root board)" \
  bash -c "cd deep/nested && '${AMD}' board | grep -q TODO"

echo "create:"
"${AMD}" new "First task" >/dev/null
"${AMD}" new "Second task" -t x -t y >/dev/null
git add -A; git commit -qm seed
assert "creates tasks/todo/001-first-task.md" test -f tasks/todo/001-first-task.md
assert "title in frontmatter" grep -q '^title: "First task"$' tasks/todo/001-first-task.md
assert "tags in frontmatter" grep -q '^tags: \[x,y\]$' tasks/todo/002-second-task.md
assert "type label defaults to feat" grep -q '^type: "feat"$' tasks/todo/001-first-task.md
assert "branch is recorded on the ticket" grep -q '^branch: "feature/first-task"$' tasks/todo/001-first-task.md
assert "created date is filled in" grep -qE '^created: "[0-9]{4}-[0-9]{2}-[0-9]{2}"$' tasks/todo/001-first-task.md

echo "parents and the tree:"
"${AMD}" new "Guest checkout" --type fix >/dev/null
"${AMD}" new "Address form" --parent 3 >/dev/null
assert "the child records its parent id" grep -q '^parent: "003"$' tasks/todo/004-address-form.md
assert "the child links back to the parent" \
  bash -c "grep -q '^\[\[003-guest-checkout\]\]$' tasks/todo/004-address-form.md"
assert "the parent lists the child" \
  bash -c "grep -q '^- \[\[004-address-form\]\]$' tasks/todo/003-guest-checkout.md"
assert "a ticket with no parent has an empty parent field" \
  bash -c "grep -q '^parent: \"\"$' tasks/todo/001-first-task.md"
assert "an unknown parent is rejected" bash -c "! '${AMD}' new 'Orphan' --parent 99"
assert "the board nests a child under its parent" \
  bash -c "'${AMD}' board --plain | grep -q '^    \[004\] Address form'"
assert "the parent stays at the outer level" \
  bash -c "'${AMD}' board --plain | grep -q '^  \[003\] Guest checkout'"
"${AMD}" new "Deep child" --parent 4 >/dev/null
assert "nesting goes deeper than two levels" \
  bash -c "'${AMD}' board --plain | grep -q '^      \[005\] Deep child'"
assert "epics and stories are gone" \
  bash -c "! '${AMD}' epics 2>/dev/null && ! '${AMD}' stories 2>/dev/null"

echo "board:"
assert "board shows all three columns" \
  bash -c "'${AMD}' board | grep -q TODO && '${AMD}' board | grep -q DOING && '${AMD}' board | grep -q DONE"
assert "board lists the task by id and title" \
  bash -c "'${AMD}' board | grep -q '\[001\] First task'"
assert "empty columns say so" bash -c "'${AMD}' ls doing | grep -q '(empty)'"

echo "ticket types:"
assert "tickets record their type" grep -q '^ticket: "development"$' tasks/todo/001-first-task.md
"${AMD}" new "Renew the certificates" --ticket admin >/dev/null
assert "admin tickets record their type" grep -q '^ticket: "admin"$' tasks/todo/006-renew-the-certificates.md
assert "admin tickets carry no branch" \
  bash -c "! grep -q '^branch:' tasks/todo/006-renew-the-certificates.md"
assert "admin tickets carry no change type" \
  bash -c "! grep -q '^type:' tasks/todo/006-renew-the-certificates.md"
assert "development tickets carry both" \
  bash -c "grep -q '^type: \"feat\"$' tasks/todo/001-first-task.md && grep -q '^branch: \"feature/first-task\"$' tasks/todo/001-first-task.md"
assert "templates lists both ticket types" \
  bash -c "'${AMD}' templates | grep -q '^development' && '${AMD}' templates | grep -q '^admin'"
assert "the board flags a non-development ticket" \
  bash -c "'${AMD}' board --plain | grep -q 'Renew the certificates  (admin)'"

echo "moves (git mv) + branches:"
git add -A; git commit -qm labels
"${AMD}" start 1 >/dev/null
assert "start: todo -> doing" test -f tasks/doing/001-first-task.md
assert "start creates the ticket's branch" \
  bash -c "git branch --show-current | grep -q '^feature/first-task$'"
assert "the staged move travelled to the new branch" \
  bash -c "git status --porcelain | grep -q 'tasks/doing/001-first-task.md'"
git checkout -q main 2>/dev/null || git checkout -q master
"${AMD}" start 6 >/dev/null
assert "admin work creates no branch" \
  bash -c "! git branch --list 'feature/renew-the-certificates' | grep -q ."
assert "admin work still moves to doing" test -f tasks/doing/006-renew-the-certificates.md
assert "start says why it left the branch alone" \
  bash -c "'${AMD}' back 6 >/dev/null && '${AMD}' start 6 2>&1 | grep -q \"admin tickets don't use branches\""
assert "development work does create a branch" \
  bash -c "git branch --list 'feature/first-task' | grep -q ."
"${AMD}" --no-input start 3 --no-branch >/dev/null
assert "--no-branch leaves the branch alone" \
  bash -c "! git branch --list 'bugfix/guest-checkout' | grep -q ."
assert "start: no-branch still moved the task" test -f tasks/doing/003-guest-checkout.md
"${AMD}" back 3 >/dev/null
"${AMD}" start 3 --branch spike/try-something >/dev/null
assert "--branch overrides the ticket" \
  bash -c "git branch --show-current | grep -q '^spike/try-something$'"
assert "an invalid --branch is rejected" \
  bash -c "'${AMD}' start 4 --branch 'bad branch' 2>&1 | grep -q 'cannot contain whitespace'"
git checkout -q main 2>/dev/null || git checkout -q master
"${AMD}" "done" 1 >/dev/null
assert "done: doing -> done" test -f tasks/done/001-first-task.md
assert "moved via git (rename tracked)" bash -c 'git status --porcelain | grep -q "^R"'
"${AMD}" back 1 >/dev/null
assert "back: done -> doing" test -f tasks/doing/001-first-task.md
assert "moving into the same column fails" bash -c "! '${AMD}' start 1"

echo "refs + ids:"
"${AMD}" start second >/dev/null
assert "find by slug substring" test -f tasks/doing/002-second-task.md
git checkout -q main 2>/dev/null || git checkout -q master
"${AMD}" new "Third" >/dev/null
assert "ids continue across columns (007)" test -f tasks/todo/007-third.md
assert "unknown ref fails" bash -c "! '${AMD}' show 99"
assert "ambiguous ref fails" bash -c "! '${AMD}' show task"
assert "show prints the file" bash -c "'${AMD}' show 007 | grep -q '^title: \"Third\"$'"

echo "templates:"
assert "templates says where each ticket type comes from" \
  bash -c "'${AMD}' templates | grep -q '^development  *built-in$'"
assert "unknown template fails with a hint" \
  bash -c "'${AMD}' new 'Nope' --template nope 2>&1 | grep -q 'amd templates'"
assert "eject writes an editable copy into the board" \
  bash -c "'${AMD}' templates eject development >/dev/null && test -f tasks/templates/development.md.jinja"
assert "eject refuses to clobber without --force" \
  bash -c "! '${AMD}' templates eject development"
printf -- '---\nid: {{ id | yaml }}\ntitle: {{ title | yaml }}\ntype: {{ type | yaml }}\nowner: {{ extra.owner | yaml }}\n---\n\n## Custom\n' \
  > tasks/templates/development.md.jinja
assert "board template overrides the built-in" \
  bash -c "'${AMD}' new 'Overridden' -s owner=tim >/dev/null && grep -q '^## Custom$' tasks/todo/*-overridden.md"
assert "--set values reach the template" \
  bash -c "grep -q '^owner: \"tim\"$' tasks/todo/*-overridden.md"
assert "labels reach a custom template" \
  bash -c "grep -q '^type: \"feat\"$' tasks/todo/*-overridden.md"
assert "templates list shows the board override" \
  bash -c "'${AMD}' templates | grep -q 'templates/development.md.jinja'"
printf -- '---\nid: {{ id | yaml }}\nowner: {{ extra.owner | yaml }}\ndue: {{ extra["due date"] | yaml }}\n---\n' \
  > tasks/templates/development.md.jinja
assert "a template field with no value is an error, naming the flag" \
  bash -c "'${AMD}' new 'Needs fields' 2>&1 | grep -q -- '--set owner='"
assert "every missing field is listed" \
  bash -c "'${AMD}' new 'Needs fields' 2>&1 | grep -q -- '--set due date='"
printf 'title: {{ titel }}\n' > tasks/templates/development.md.jinja
assert "a typo'd variable is an error, not a blank line" \
  bash -c "! '${AMD}' new 'Broken' 2>&1 | grep -q '^title: $'"
assert "the template error names the template and line" \
  bash -c "'${AMD}' new 'Broken' 2>&1 | grep -q \"template 'development'\""
rm -rf tasks/templates

echo "the body editor:"
assert "non-interactive create never opens an editor" \
  bash -c "EDITOR=false '${AMD}' new 'No editor here' >/dev/null && test -f tasks/todo/*-no-editor-here.md"
assert "--no-edit is accepted alongside a title" \
  bash -c "'${AMD}' new 'Also no editor' --no-edit >/dev/null"
assert "--edit and --no-edit conflict" \
  bash -c "! '${AMD}' new 'Both' --edit --no-edit"

echo "quoting:"
"${AMD}" new 'Fix the "quoted" thing' >/dev/null
assert "a quote in the title is escaped, not left to break the frontmatter" \
  bash -c "grep -qF 'title: \"Fix the \\\"quoted\\\" thing\"' tasks/todo/*-fix-the-quoted-thing.md"

echo "related tickets:"
assert "related is empty by default" \
  bash -c "grep -q '^related: \[\]$' tasks/*/001-first-task.md"
"${AMD}" new "Depends on the first" --related 1 >/dev/null
assert "--related records the id" \
  bash -c "grep -q '^related: \[001\]$' tasks/todo/*-depends-on-the-first.md"
assert "the other end is linked back" \
  bash -c "grep -qE '^related: \[0[0-9]+\]$' tasks/*/001-first-task.md"
assert "an unknown related ref is rejected at creation" \
  bash -c "! '${AMD}' new 'Dangling' --related 99"
assert "amd link relates two tickets both ways" \
  bash -c "'${AMD}' link 2 depends-on-the-first >/dev/null \
    && grep -q 'related: \[0' tasks/*/002-second-task.md \
    && grep -q '002' tasks/todo/*-depends-on-the-first.md"
assert "amd link is idempotent" \
  bash -c "'${AMD}' link 2 depends-on-the-first | grep -q 'already related'"
assert "a task cannot be related to itself" \
  bash -c "'${AMD}' link 2 2 2>&1 | grep -q \"can't be related to itself\""
"${AMD}" new "One way only" >/dev/null
assert "amd link --one-way leaves the other end alone" \
  bash -c "'${AMD}' link 2 one-way --one-way >/dev/null && ! grep -q '002' tasks/todo/*-one-way-only.md"
assert "admin tickets carry the list too" \
  bash -c "grep -q '^related: \[\]$' tasks/*/006-renew-the-certificates.md"

echo "completions:"
assert "bash completions are valid bash" \
  bash -c "'${AMD}' completions bash 2>/dev/null > comp.bash && bash -n comp.bash"
assert "zsh completions start with the compdef line" \
  bash -c "'${AMD}' completions zsh 2>/dev/null | head -1 | grep -q '^#compdef amd$'"
assert "fish completions are fish syntax" \
  bash -c "'${AMD}' completions fish 2>/dev/null | grep -q '^complete -c amd'"
assert "completions know the subcommands" \
  bash -c "'${AMD}' completions bash 2>/dev/null | grep -q 'link' && '${AMD}' completions fish 2>/dev/null | grep -q 'templates'"
assert "completions offer the change types" \
  bash -c "'${AMD}' completions fish 2>/dev/null | grep -q 'feat'"
assert "completions offer the ticket types" \
  bash -c "'${AMD}' completions fish 2>/dev/null | grep -q 'development'"
assert "the shell is taken from \$SHELL when not given" \
  bash -c "SHELL=/bin/zsh '${AMD}' completions 2>/dev/null | head -1 | grep -q '^#compdef amd$'"
assert "an unknown shell errors with the choices" \
  bash -c "SHELL=/bin/nope '${AMD}' completions 2>&1 | grep -q 'amd completions bash|zsh|fish'"
assert "the install hint goes to stderr, not into the script" \
  bash -c "'${AMD}' completions bash 2>/dev/null | grep -qv '^# install it with:' && '${AMD}' completions bash 2>&1 >/dev/null | grep -q 'install it with'"
assert "completions work without a board" \
  bash -c "cd '${nongit}' && '${AMD}' completions bash | grep -q '_amd'"
rm -f comp.bash

echo "guards:"
assert "errors outside a git repository" \
  bash -c "cd '${nongit}' && ! '${AMD}' board"

echo "non-interactive (--no-input / AMD_NO_INPUT):"
assert "new with no title errors instead of prompting" \
  bash -c "! '${AMD}' new"
assert "new with no title shows the usage line" \
  bash -c "'${AMD}' new 2>&1 | grep -q 'usage: amd new'"
assert "start with no ref errors instead of prompting" \
  bash -c "'${AMD}' start 2>&1 | grep -q 'usage: amd start <ref>'"
assert "--no-input works without the env var" \
  bash -c "env -u AMD_NO_INPUT '${AMD}' --no-input show 2>&1 | grep -q 'usage: amd show <ref>'"

echo "AMD_DIR:"
assert "AMD_DIR relocates the board" \
  bash -c "cd '${tmp}' && AMD_DIR=work AMD_YES=1 '${AMD}' new 'Elsewhere' >/dev/null && test -f work/todo/001-elsewhere.md"

echo "auto-create when no board exists:"
fresh="$(mktemp -d)"; ( cd "${fresh}" && git init -q )
assert "non-interactive with no board errors (no hang)" \
  bash -c "cd '${fresh}' && ! '${AMD}' board </dev/null"
assert "AMD_YES=1 creates the board and runs" \
  bash -c "cd '${fresh}' && AMD_YES=1 '${AMD}' board >/dev/null && test -d tasks/todo"
rm -rf "${fresh}"

echo
echo "passed: ${pass}, failed: ${fail}"
[ "${fail}" -eq 0 ]
