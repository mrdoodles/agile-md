#!/usr/bin/env bash
#
# agile-md installer — install the `task` command onto your PATH.
#
#   ./install.sh [--dir DIR]     # default: /usr/local/bin if writable, else ~/bin
#   curl -fsSL https://raw.githubusercontent.com/mrdoodles/agile-md/v2/install.sh | bash
#
# Then, in any git repository:  task init  &&  task new "My first task"
#
set -euo pipefail

DIR=""
while [ $# -gt 0 ]; do
  case "$1" in
    --dir) DIR="$2"; shift 2 ;;
    *) shift ;;
  esac
done
if [ -z "${DIR}" ]; then
  if [ -d /usr/local/bin ] && [ -w /usr/local/bin ]; then
    DIR="/usr/local/bin"
  else
    DIR="${HOME}/bin"
  fi
fi

SELF="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RAW="https://raw.githubusercontent.com/mrdoodles/agile-md/v2"

mkdir -p "${DIR}"
if [ -f "${SELF}/task" ]; then
  cp "${SELF}/task" "${DIR}/task"
else
  curl -fsSL "${RAW}/task" -o "${DIR}/task"
fi
chmod +x "${DIR}/task"

echo "Installed task -> ${DIR}/task"
case ":${PATH}:" in
  *":${DIR}:"*) ;;
  *) echo "Note: ${DIR} is not on your PATH. Add this to your shell profile:"
     echo "      export PATH=\"${DIR}:\$PATH\"" ;;
esac
echo "Then, in any repo:  task init  &&  task new \"My first task\""
