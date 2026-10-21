#!/usr/bin/env bash
# Starts the firmware already in flash, without touching BUT.
#
# BUT pulls PF4/BOOT0 high, so a plain reset lands in the system memory
# bootloader. Halt at reset and point the core at the flash reset vector
# instead (SP and VTOR are set by the firmware itself).
#
# HOLD=1 keeps the debugger attached until Ctrl-C. Use it when the probe's
# reset line reaches nRST: such a probe resets the chip when it disconnects.
set -euo pipefail

end="shutdown"
[[ "${HOLD:-0}" == "1" ]] && end=""

openocd -f interface/cmsis-dap.cfg -c "
  transport select swd; adapter speed ${SPEED_KHZ:-100}
  swd newdap chip cpu -enable
  dap create chip.dap -chain-position chip.cpu
  target create chip.cpu cortex_m -dap chip.dap
  init; reset halt
  reg pc [expr {[read_memory 0x08000004 32 1] & ~1}]
  resume; ${end}"
