#!/usr/bin/env bash
# Rechecks the SWD connection until interrupted (Ctrl-C), one line per try.
# The last column counts consecutive good reads: the link is stable once it
# keeps climbing.
set -uo pipefail

CHIP="${CHIP:-PY32F030x8}"
IDCODE=60001000

streak=0
ok=0
total=0
trap 'echo; echo "${ok}/${total} ok"; exit 0' INT

while true; do
  id=$(probe-rs read --chip "${CHIP}" b32 0x40015800 1 2>/dev/null | tr -d ' \n')
  total=$((total + 1))
  if [[ "${id}" == "${IDCODE}" ]]; then
    ok=$((ok + 1))
    streak=$((streak + 1))
    printf '%s  \e[32mOK  \e[0m  streak %d\n' "$(date +%T)" "${streak}"
  else
    streak=0
    printf '%s  \e[31mFAIL\e[0m\n' "$(date +%T)"
  fi
  sleep 0.3
done
