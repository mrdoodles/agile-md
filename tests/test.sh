#!/usr/bin/env bash
#
# End-to-end spec for the amd CLI. Run: bash tests/test.sh
#
# Builds the binary with cargo, then drives it against throwaway git repos.
# Unit tests for the pure logic (slugify, frontmatter, templates) live in the
# Rust sources and run under `cargo test`.
#
set -uo pipefail

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
assert "created date is filled in" grep -qE '^created: "[0-9]{4}-[0-9]{2}-[0-9]{2}"$' tasks/todo/001-first-task.md

echo "board:"
assert "board shows all three columns" \
  bash -c "'${AMD}' board | grep -q TODO && '${AMD}' board | grep -q DOING && '${AMD}' board | grep -q DONE"
assert "board lists the task by id and title" \
  bash -c "'${AMD}' board | grep -q '\[001\] First task'"
assert "empty columns say so" bash -c "'${AMD}' ls doing | grep -q '(empty)'"

echo "moves (git mv):"
"${AMD}" start 1 >/dev/null
assert "start: todo -> doing" test -f tasks/doing/001-first-task.md
"${AMD}" "done" 1 >/dev/null
assert "done: doing -> done" test -f tasks/done/001-first-task.md
assert "moved via git (rename tracked)" bash -c 'git status --porcelain | grep -q "^R"'
"${AMD}" back 1 >/dev/null
assert "back: done -> doing" test -f tasks/doing/001-first-task.md
assert "moving into the same column fails" bash -c "! '${AMD}' start 1"

echo "refs + ids:"
"${AMD}" start second >/dev/null
assert "find by slug substring" test -f tasks/doing/002-second-task.md
"${AMD}" new "Third" >/dev/null
assert "ids continue across columns (003)" test -f tasks/todo/003-third.md
assert "unknown ref fails" bash -c "! '${AMD}' show 99"
assert "ambiguous ref fails" bash -c "! '${AMD}' show task"
assert "show prints the file" bash -c "'${AMD}' show 003 | grep -q '^title: \"Third\"$'"

echo "templates:"
assert "templates lists the built-ins" \
  bash -c "'${AMD}' templates | grep -q '^task  *built-in$'"
assert "new --template bug uses the bug template" \
  bash -c "'${AMD}' new 'Crash on save' --template bug >/dev/null && grep -q '^type: bug$' tasks/todo/004-crash-on-save.md"
assert "unknown template fails with a hint" \
  bash -c "'${AMD}' new 'Nope' --template nope 2>&1 | grep -q 'amd templates'"
assert "eject writes an editable copy into the board" \
  bash -c "'${AMD}' templates eject task >/dev/null && test -f tasks/templates/task.md.jinja"
assert "eject refuses to clobber without --force" \
  bash -c "! '${AMD}' templates eject task"
printf -- '---\nid: {{ id | yaml }}\ntitle: {{ title | yaml }}\nowner: {{ extra.owner | yaml }}\n---\n\n## Custom\n' \
  > tasks/templates/task.md.jinja
assert "board template overrides the built-in" \
  bash -c "'${AMD}' new 'Overridden' -s owner=tim >/dev/null && grep -q '^## Custom$' tasks/todo/005-overridden.md"
assert "--set values reach the template" grep -q '^owner: "tim"$' tasks/todo/005-overridden.md
assert "templates list shows the board override" \
  bash -c "'${AMD}' templates | grep -q 'templates/task.md.jinja'"
printf 'title: {{ titel }}\n' > tasks/templates/task.md.jinja
assert "a typo'd variable is an error, not a blank line" \
  bash -c "! '${AMD}' new 'Broken' 2>&1 | grep -q '^title: $'"
assert "the template error names the template and line" \
  bash -c "'${AMD}' new 'Broken' 2>&1 | grep -q \"template 'task'\""
rm -rf tasks/templates

echo "quoting:"
"${AMD}" new 'Fix the "quoted" thing' >/dev/null
assert "a quote in the title is escaped, not left to break the frontmatter" \
  grep -qF 'title: "Fix the \"quoted\" thing"' tasks/todo/006-fix-the-quoted-thing.md

echo "guards:"
assert "errors outside a git repository" \
  bash -c "cd '${nongit}' && ! '${AMD}' board"

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
