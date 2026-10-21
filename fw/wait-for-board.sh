#!/usr/bin/env bash
# Polls SWD until the MCU answers with the PY32F030 device ID.
# Exits 0 once it does, 1 after TIMEOUT seconds (default 90).
set -uo pipefail

CHIP="${CHIP:-PY32F030x8}"
TIMEOUT="${TIMEOUT:-90}"
IDCODE=60001000

deadline=$((SECONDS + TIMEOUT))
while ((SECONDS < deadline)); do
  id=$(probe-rs read --chip "${CHIP}" b32 0x40015800 1 2>/dev/null | tr -d ' \n')
  if [[ "${id}" == "${IDCODE}" ]]; then
    echo "board detected after ${SECONDS}s"
    exit 0
  fi
  sleep 0.5
done

echo "no board after ${TIMEOUT}s" >&2
exit 1
