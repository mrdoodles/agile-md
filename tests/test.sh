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

echo "labels:"
"${AMD}" new "Guest checkout" --type fix --epic checkout --story guest >/dev/null
assert "type label is stored" grep -q '^type: "fix"$' tasks/todo/003-guest-checkout.md
assert "epic label is stored" grep -q '^epic: "checkout"$' tasks/todo/003-guest-checkout.md
assert "story label is stored" grep -q '^story: "guest"$' tasks/todo/003-guest-checkout.md
assert "fix tickets get a bugfix branch" grep -q '^branch: "bugfix/guest-checkout"$' tasks/todo/003-guest-checkout.md
assert "docs tickets get a chore branch" \
  bash -c "'${AMD}' new 'Write the guide' --type docs >/dev/null && grep -q '^branch: \"chore/write-the-guide\"\$' tasks/todo/004-write-the-guide.md"
assert "an unknown type is rejected, listing the valid ones" \
  bash -c "'${AMD}' new 'Nope' --type feature 2>&1 | grep -q \"unknown type 'feature'\""
assert "AMD_TYPES overrides the list" \
  bash -c "AMD_TYPES='spike,feat' '${AMD}' new 'Try it' --type spike >/dev/null && grep -q '^type: \"spike\"\$' tasks/todo/005-try-it.md"
assert "a title that cannot become a branch is rejected" \
  bash -c "'${AMD}' new '***' 2>&1 | grep -q 'no letters or numbers'"
assert "epics lists the epic with progress" bash -c "'${AMD}' epics | grep -q '^checkout  0/1 done$'"
assert "stories lists the story" bash -c "'${AMD}' stories | grep -q '^guest  0/1 done$'"
assert "epics <name> lists that epic's tasks" bash -c "'${AMD}' epics checkout | grep -q 'Guest checkout'"
assert "an unknown epic errors" bash -c "! '${AMD}' epics nope"
assert "the board shows the labels" bash -c "'${AMD}' board | grep -q 'Guest checkout  (fix epic:checkout story:guest)'"

echo "board:"
assert "board shows all three columns" \
  bash -c "'${AMD}' board | grep -q TODO && '${AMD}' board | grep -q DOING && '${AMD}' board | grep -q DONE"
assert "board lists the task by id and title" \
  bash -c "'${AMD}' board | grep -q '\[001\] First task'"
assert "empty columns say so" bash -c "'${AMD}' ls doing | grep -q '(empty)'"

echo "scopes:"
assert "scope defaults to code" grep -q '^scope: "code"$' tasks/todo/001-first-task.md
"${AMD}" new "Update the rota" --scope admin >/dev/null
assert "admin scope is stored" grep -q '^scope: "admin"$' tasks/todo/006-update-the-rota.md
assert "admin scope tickets carry no branch" grep -q '^branch: ""$' tasks/todo/006-update-the-rota.md
assert "an unknown scope is rejected, listing the valid ones" \
  bash -c "'${AMD}' new 'Nope' --scope nope 2>&1 | grep -q \"unknown scope 'nope'\""
assert "AGILE_MD_SCOPES adds to the scope list" \
  bash -c "AGILE_MD_SCOPES=docs '${AMD}' new 'Nope' --scope nope 2>&1 | grep -q 'code, admin, docs'"
assert "AGILE_MD_SCOPES makes the extra scope usable" \
  bash -c "AGILE_MD_SCOPES=docs '${AMD}' new 'Write it up' --scope docs >/dev/null && grep -q '^scope: \"docs\"\$' tasks/todo/007-write-it-up.md"
assert "the board shows a non-default scope" \
  bash -c "'${AMD}' board | grep -q 'Update the rota  (feat admin)'"

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
assert "admin scope work creates no branch" \
  bash -c "! git branch --list 'chore/update-the-rota' | grep -q ."
assert "admin scope work still moves to doing" test -f tasks/doing/006-update-the-rota.md
assert "start says why it left the branch alone" \
  bash -c "'${AMD}' back 6 >/dev/null && '${AMD}' start 6 2>&1 | grep -q \"admin scope work doesn't use branches\""
assert "code scope work does create a branch" \
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
assert "ids continue across columns (008)" test -f tasks/todo/008-third.md
assert "unknown ref fails" bash -c "! '${AMD}' show 99"
assert "ambiguous ref fails" bash -c "! '${AMD}' show task"
assert "show prints the file" bash -c "'${AMD}' show 008 | grep -q '^title: \"Third\"$'"

echo "templates:"
assert "templates lists the built-in task template" \
  bash -c "'${AMD}' templates | grep -q '^task  *built-in$'"
assert "unknown template fails with a hint" \
  bash -c "'${AMD}' new 'Nope' --template nope 2>&1 | grep -q 'amd templates'"
assert "eject writes an editable copy into the board" \
  bash -c "'${AMD}' templates eject task >/dev/null && test -f tasks/templates/task.md.jinja"
assert "eject refuses to clobber without --force" \
  bash -c "! '${AMD}' templates eject task"
printf -- '---\nid: {{ id | yaml }}\ntitle: {{ title | yaml }}\ntype: {{ type | yaml }}\nepic: {{ epic | yaml }}\nowner: {{ extra.owner | yaml }}\n---\n\n## Custom\n' \
  > tasks/templates/task.md.jinja
assert "board template overrides the built-in" \
  bash -c "'${AMD}' new 'Overridden' -s owner=tim --epic checkout >/dev/null && grep -q '^## Custom$' tasks/todo/*-overridden.md"
assert "--set values reach the template" \
  bash -c "grep -q '^owner: \"tim\"$' tasks/todo/*-overridden.md"
assert "labels reach a custom template" \
  bash -c "grep -q '^epic: \"checkout\"$' tasks/todo/*-overridden.md"
assert "templates list shows the board override" \
  bash -c "'${AMD}' templates | grep -q 'templates/task.md.jinja'"
printf -- '---\nid: {{ id | yaml }}\nowner: {{ extra.owner | yaml }}\ndue: {{ extra["due date"] | yaml }}\n---\n' \
  > tasks/templates/task.md.jinja
assert "a template field with no value is an error, naming the flag" \
  bash -c "'${AMD}' new 'Needs fields' 2>&1 | grep -q -- '--set owner='"
assert "every missing field is listed" \
  bash -c "'${AMD}' new 'Needs fields' 2>&1 | grep -q -- '--set due date='"
printf 'title: {{ titel }}\n' > tasks/templates/task.md.jinja
assert "a typo'd variable is an error, not a blank line" \
  bash -c "! '${AMD}' new 'Broken' 2>&1 | grep -q '^title: $'"
assert "the template error names the template and line" \
  bash -c "'${AMD}' new 'Broken' 2>&1 | grep -q \"template 'task'\""
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
