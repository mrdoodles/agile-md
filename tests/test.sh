#!/usr/bin/env bash
#
# Exercises the amd CLI. Run: bash tests/test.sh
#
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
AMD="${ROOT}/amd"

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
bash "${AMD}" init >/dev/null
assert "init creates todo/doing/done" test -d tasks/todo -a -d tasks/doing -a -d tasks/done
assert "init creates an archive that keeps itself out of git" \
  bash -c "test -f tasks/archive/.gitignore && grep -q '^\*$' tasks/archive/.gitignore"

echo "discovery from a subdirectory:"
mkdir -p deep/nested
assert "runs from a subdir (finds repo-root board)" \
  bash -c "cd deep/nested && bash '${AMD}' board | grep -q TODO"

echo "create:"
bash "${AMD}" new "First task" >/dev/null
bash "${AMD}" new "Second task" -t x -t y >/dev/null
git add -A; git commit -qm seed
assert "creates tasks/todo/001-first-task.md" test -f tasks/todo/001-first-task.md
assert "title in frontmatter" grep -q '^title: "First task"$' tasks/todo/001-first-task.md
assert "tags in frontmatter" grep -q '^tags: \[x,y\]$' tasks/todo/002-second-task.md

echo "board:"
assert "board shows all three columns" \
  bash -c "bash '${AMD}' board | grep -q TODO && bash '${AMD}' board | grep -q DOING && bash '${AMD}' board | grep -q DONE"

echo "moves (git mv):"
bash "${AMD}" start 1 >/dev/null
assert "start: todo -> doing" test -f tasks/doing/001-first-task.md
bash "${AMD}" "done" 1 >/dev/null
assert "done: doing -> done" test -f tasks/done/001-first-task.md
assert "moved via git (rename tracked)" bash -c 'git status --porcelain | grep -q "^R"'
bash "${AMD}" back 1 >/dev/null
assert "back: done -> doing" test -f tasks/doing/001-first-task.md

echo "refs + ids:"
bash "${AMD}" start second >/dev/null
assert "find by slug substring" test -f tasks/doing/002-second-task.md
bash "${AMD}" new "Third" >/dev/null
assert "ids continue across columns (003)" test -f tasks/todo/003-third.md

echo "archive:"
bash "${AMD}" new "Archive me" >/dev/null
git add -A; git commit -qm archivable
archived_file="$(basename tasks/todo/*-archive-me.md)"
archived_id="${archived_file%%-*}"
assert "archive takes a task off the board" \
  bash -c "bash '${AMD}' archive archive-me | grep -q 'archived'"
assert "the file is in archive/" test -f "tasks/archive/${archived_id}-archive-me.md"
assert "the board no longer shows it" \
  bash -c "! bash '${AMD}' board | grep -q 'Archive me'"
assert "ls archive still lists it" \
  bash -c "bash '${AMD}' ls archive | grep -q 'Archive me'"
assert "git records the task leaving the board" \
  bash -c "git status --porcelain | grep -q '^D  tasks/todo/.*archive-me'"
assert "the archive itself stays out of git" \
  bash -c "! git status --porcelain | grep -q 'tasks/archive/0'"
assert "an archived task is no longer findable" bash -c "! bash '${AMD}' show archive-me"
assert "a bad ref reports once and stops" \
  bash -c "test \"\$(bash '${AMD}' show 999 2>&1 | wc -l)\" -eq 1"
assert "ids do not go back after archiving" \
  bash -c "bash '${AMD}' new 'After archiving' >/dev/null; ! test -f tasks/todo/${archived_id}-after-archiving.md"

echo "archiving on a board that predates archive/:"
legacy="$(mktemp -d)"
(
  cd "${legacy}" || exit 1
  git init -q; git config user.email t@t.co; git config user.name t
  mkdir -p tasks/todo tasks/doing tasks/done          # no archive/, as v2 left it
  printf -- '---\nid: "001"\ntitle: "Old"\ncreated: "2026-01-01"\ntags: []\n---\n' \
    > tasks/todo/001-old.md
  git add -A; git commit -qm seed
  bash "${AMD}" archive 1 >/dev/null
)
cd "${legacy}" || exit 1
assert "the archive is created on first use" test -f tasks/archive/001-old.md
assert "and it brings its .gitignore with it" \
  bash -c "test -f tasks/archive/.gitignore && grep -q '^\*\$' tasks/archive/.gitignore"
assert "so the archived task never becomes untracked content" \
  bash -c "! git status --porcelain --untracked-files=all | grep -q 'tasks/archive/001'"
rm -f tasks/archive/.gitignore
assert "any command puts a deleted .gitignore back" \
  bash -c "bash '${AMD}' board >/dev/null && test -f tasks/archive/.gitignore"
assert "and the archived tasks are still ignored" \
  bash -c "! git status --porcelain --untracked-files=all | grep -q 'tasks/archive/001'"
assert "restoring it does not disturb the archived tasks" test -f tasks/archive/001-old.md
cd "${tmp}" || exit 1
rm -rf "${legacy}"

echo "clean:"
assert "clean refuses to delete unprompted" \
  bash -c "! bash '${AMD}' clean </dev/null"
assert "and deletes nothing when it refuses" test -f "tasks/archive/${archived_id}-archive-me.md"
assert "AMD_YES=1 empties the archive" \
  bash -c "AMD_YES=1 bash '${AMD}' clean | grep -q 'deleted 1'"
assert "the tasks are gone" bash -c "! test -f tasks/archive/${archived_id}-archive-me.md"
assert "but the .gitignore is not" test -f tasks/archive/.gitignore
assert "cleaning an empty archive says so" \
  bash -c "AMD_YES=1 bash '${AMD}' clean | grep -q 'already empty'"
assert "cleanup is the same command" \
  bash -c "AMD_YES=1 bash '${AMD}' cleanup | grep -q 'already empty'"

echo "guards:"
assert "errors outside a git repository" \
  bash -c "cd '${nongit}' && ! bash '${AMD}' board"

echo "auto-create when no board exists:"
fresh="$(mktemp -d)"; ( cd "${fresh}" && git init -q )
assert "non-interactive with no board errors (no hang)" \
  bash -c "cd '${fresh}' && ! bash '${AMD}' board </dev/null"
assert "AMD_YES=1 creates the board and runs" \
  bash -c "cd '${fresh}' && AMD_YES=1 bash '${AMD}' board >/dev/null && test -d tasks/todo"
rm -rf "${fresh}"

echo
echo "passed: ${pass}, failed: ${fail}"
[ "${fail}" -eq 0 ]
