#!/usr/bin/env bash
#
# agile-md installer — install the `amd` command onto your PATH.
#
#   ./install.sh [--dir DIR]     # default: /usr/local/bin if writable, else ~/bin
#   curl -fsSL https://raw.githubusercontent.com/mrdoodles/agile-md/v3/install.sh | bash
#
# Then, in any git repository:  amd init  &&  amd new "My first task"
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
RAW="https://raw.githubusercontent.com/mrdoodles/agile-md/v3"

mkdir -p "${DIR}"
if [ -f "${SELF}/amd" ]; then
  cp "${SELF}/amd" "${DIR}/amd"
else
  curl -fsSL "${RAW}/amd" -o "${DIR}/amd"
fi
chmod +x "${DIR}/amd"

echo "Installed amd -> ${DIR}/amd"
case ":${PATH}:" in
  *":${DIR}:"*) ;;
  *) echo "Note: ${DIR} is not on your PATH. Add this to your shell profile:"
     echo "      export PATH=\"${DIR}:\$PATH\"" ;;
esac
echo "Then, in any repo:  amd init  &&  amd new \"My first task\""
