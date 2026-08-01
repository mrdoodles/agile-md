#!/usr/bin/env bash
#
# agile-md installer — scaffold a markdown task board in the current repository.
#
#   ./install.sh [board-dir]      # from a clone (default board-dir: tasks)
#   curl -fsSL https://raw.githubusercontent.com/mrdoodles/agile-md/v1/install.sh | bash
#
set -euo pipefail

DIR="${1:-tasks}"
SELF="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RAW="https://raw.githubusercontent.com/mrdoodles/agile-md/v1"

mkdir -p "${DIR}/todo" "${DIR}/doing" "${DIR}/done"
for c in todo doing "done"; do
  [ -e "${DIR}/${c}/.gitkeep" ] || : > "${DIR}/${c}/.gitkeep"
done

# Vendor the task script from this checkout, or fetch it.
if [ -f "${SELF}/task" ]; then
  cp "${SELF}/task" "${DIR}/task"
else
  curl -fsSL "${RAW}/task" -o "${DIR}/task"
fi
chmod +x "${DIR}/task"

if [ ! -e "${DIR}/README.md" ]; then
  cat > "${DIR}/README.md" <<EOF
# Tasks (agile-md)

A filesystem Kanban: markdown tasks moved between \`todo/\`, \`doing/\` and
\`done/\`. The column is the status; \`git mv\` between them is the audit trail.

\`\`\`bash
${DIR}/task new "My task"   # create in todo/
${DIR}/task board           # show the board
${DIR}/task start <ref>     # todo  -> doing
${DIR}/task done  <ref>     # doing -> done
${DIR}/task back  <ref>     # move one column left
${DIR}/task show  <ref>     # print a task
\`\`\`

\`<ref>\` is a task id (e.g. \`7\`) or a unique slug substring.
See https://github.com/mrdoodles/agile-md for details.
EOF
fi

echo "Initialised agile-md board in ./${DIR}"
echo "Try: ${DIR}/task new \"My first task\""
