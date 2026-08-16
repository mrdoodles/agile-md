#!/usr/bin/env bash
#
# agile-md installer — install the `amd` command onto your PATH.
#
#   ./install.sh [--dir DIR]     # default: /usr/local/bin if writable, else ~/bin
#   ./install.sh --with-skill    # also install the Claude Code skill
#   curl -fsSL https://raw.githubusercontent.com/mrdoodles/agile-md/v4/install.sh | bash
#
# Then, in any git repository:  amd init  &&  amd new "My first task"
#
set -euo pipefail

DIR=""
WITH_SKILL=0
while [ $# -gt 0 ]; do
  case "$1" in
    --dir) DIR="$2"; shift 2 ;;
    --with-skill) WITH_SKILL=1; shift ;;
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
RAW="https://raw.githubusercontent.com/mrdoodles/agile-md/v4"

mkdir -p "${DIR}"
if [ -f "${SELF}/amd" ]; then
  cp "${SELF}/amd" "${DIR}/amd"
else
  curl -fsSL "${RAW}/amd" -o "${DIR}/amd"
fi
chmod +x "${DIR}/amd"

echo "Installed amd -> ${DIR}/amd"

# Opt-in: the skill teaches Claude Code how to drive a board. Installing it is
# not part of installing the tool — it writes into another tool's config, and
# plenty of amd users have no Claude Code at all.
if [ "${WITH_SKILL}" = "1" ]; then
  SKILL_REL=".claude/skills/agile-md/SKILL.md"
  SKILL_DIR="${HOME}/.claude/skills/agile-md"
  mkdir -p "${SKILL_DIR}"
  if [ -f "${SELF}/${SKILL_REL}" ]; then
    cp "${SELF}/${SKILL_REL}" "${SKILL_DIR}/SKILL.md"
  else
    curl -fsSL "${RAW}/${SKILL_REL}" -o "${SKILL_DIR}/SKILL.md"
  fi
  echo "Installed skill -> ${SKILL_DIR}/SKILL.md"
fi
case ":${PATH}:" in
  *":${DIR}:"*) ;;
  *) echo "Note: ${DIR} is not on your PATH. Add this to your shell profile:"
     echo "      export PATH=\"${DIR}:\$PATH\"" ;;
esac
echo "Then, in any repo:  amd init  &&  amd new \"My first task\""
