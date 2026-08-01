#!/usr/bin/env bash
#
# Exercises the task CLI. Run: bash tests/test.sh
#
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TASK="${ROOT}/task"

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
bash "${TASK}" init >/dev/null
assert "init creates todo/doing/done" test -d tasks/todo -a -d tasks/doing -a -d tasks/done

echo "discovery from a subdirectory:"
mkdir -p deep/nested
assert "runs from a subdir (finds repo-root board)" \
  bash -c "cd deep/nested && bash '${TASK}' board | grep -q TODO"

echo "create:"
bash "${TASK}" new "First task" >/dev/null
bash "${TASK}" new "Second task" -t x -t y >/dev/null
git add -A; git commit -qm seed
assert "creates tasks/todo/001-first-task.md" test -f tasks/todo/001-first-task.md
assert "title in frontmatter" grep -q '^title: "First task"$' tasks/todo/001-first-task.md
assert "tags in frontmatter" grep -q '^tags: \[x,y\]$' tasks/todo/002-second-task.md

echo "board:"
assert "board shows all three columns" \
  bash -c "bash '${TASK}' board | grep -q TODO && bash '${TASK}' board | grep -q DOING && bash '${TASK}' board | grep -q DONE"

echo "moves (git mv):"
bash "${TASK}" start 1 >/dev/null
assert "start: todo -> doing" test -f tasks/doing/001-first-task.md
bash "${TASK}" "done" 1 >/dev/null
assert "done: doing -> done" test -f tasks/done/001-first-task.md
assert "moved via git (rename tracked)" bash -c 'git status --porcelain | grep -q "^R"'
bash "${TASK}" back 1 >/dev/null
assert "back: done -> doing" test -f tasks/doing/001-first-task.md

echo "refs + ids:"
bash "${TASK}" start second >/dev/null
assert "find by slug substring" test -f tasks/doing/002-second-task.md
bash "${TASK}" new "Third" >/dev/null
assert "ids continue across columns (003)" test -f tasks/todo/003-third.md

echo "guards:"
assert "errors outside a git repository" \
  bash -c "cd '${nongit}' && ! bash '${TASK}' board"

echo
echo "passed: ${pass}, failed: ${fail}"
[ "${fail}" -eq 0 ]
