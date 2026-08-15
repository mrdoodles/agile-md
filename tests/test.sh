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

echo "edit (MARKDOWN_EDITOR, then EDITOR):"
# Fake editors that record which one ran, and with what.
bin="$(mktemp -d)"
for name in md-ed plain-ed; do
  printf '#!/usr/bin/env bash\nprintf "%s %%s\\n" "$*" > "%s/ran"\n' "${name}" "${bin}" \
    > "${bin}/${name}"
  chmod +x "${bin}/${name}"
done
# Runs `amd edit 3` with the given environment and prints what actually ran.
# -u clears whatever the developer has set, so the result is the script's doing;
# the board path is trimmed back to a relative one (git resolves symlinks, so
# the absolute path isn't the one mktemp handed us).
edited_by() {
  rm -f "${bin}/ran"
  env -u MARKDOWN_EDITOR -u EDITOR PATH="${bin}:${PATH}" "$@" \
    bash "${AMD}" edit 3 >/dev/null 2>&1 || true
  if [ -f "${bin}/ran" ]; then sed 's#[^ ]*/tasks/#tasks/#' "${bin}/ran"
  else printf 'nothing ran\n'; fi
}
task3="tasks/todo/003-third.md"

assert "MARKDOWN_EDITOR opens the task" \
  test "$(edited_by MARKDOWN_EDITOR=md-ed EDITOR=plain-ed)" = "md-ed ${task3}"
assert "an unset MARKDOWN_EDITOR falls back to EDITOR" \
  test "$(edited_by EDITOR=plain-ed)" = "plain-ed ${task3}"
assert "an empty MARKDOWN_EDITOR falls back to EDITOR" \
  test "$(edited_by MARKDOWN_EDITOR= EDITOR=plain-ed)" = "plain-ed ${task3}"
assert "a MARKDOWN_EDITOR this machine hasn't got falls back to EDITOR" \
  test "$(edited_by MARKDOWN_EDITOR=no-such-editor EDITOR=plain-ed)" = "plain-ed ${task3}"
assert "editor arguments are honoured" \
  test "$(edited_by MARKDOWN_EDITOR='md-ed --wait')" = "md-ed --wait ${task3}"
rm -rf "${bin}"

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
