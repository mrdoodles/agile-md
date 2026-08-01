#!/usr/bin/env bash
#
# Exercises the task CLI. Run: bash tests/test.sh
#
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

pass=0
fail=0
assert() { # description  command...
  local d="$1"; shift
  if "$@" >/dev/null 2>&1; then echo "  ok   - ${d}"; pass=$((pass + 1))
  else echo "  FAIL - ${d}"; fail=$((fail + 1)); fi
}

tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT
mkdir -p "${tmp}/todo" "${tmp}/doing" "${tmp}/done"
cp "${ROOT}/task" "${tmp}/task"; chmod +x "${tmp}/task"
cd "${tmp}" || exit 1
git init -q; git config user.email t@t.co; git config user.name t

echo "create:"
./task new "First task" >/dev/null
./task new "Second task" -t x -t y >/dev/null
git add -A; git commit -qm seed
assert "creates todo/001-first-task.md" test -f todo/001-first-task.md
assert "creates todo/002-second-task.md" test -f todo/002-second-task.md
assert "title in frontmatter" grep -q '^title: "First task"$' todo/001-first-task.md
assert "tags in frontmatter" grep -q '^tags: \[x,y\]$' todo/002-second-task.md

echo "board:"
assert "board shows all three columns" \
  bash -c './task board | grep -q TODO && ./task board | grep -q DOING && ./task board | grep -q DONE'

echo "moves (git mv):"
./task start 1 >/dev/null
assert "start: todo -> doing" test -f doing/001-first-task.md
./task "done" 1 >/dev/null
assert "done: doing -> done" test -f done/001-first-task.md
./task back 1 >/dev/null
assert "back: done -> doing" test -f doing/001-first-task.md
assert "moved via git (rename tracked)" bash -c 'git -C . status --porcelain | grep -q "^R"'

echo "refs + ids:"
./task start second >/dev/null
assert "find by slug substring" test -f doing/002-second-task.md
./task new "Third" >/dev/null
assert "ids continue across columns (003)" test -f todo/003-third.md

echo
echo "passed: ${pass}, failed: ${fail}"
[ "${fail}" -eq 0 ]
