#! /bin/bash

# Invoke GDB, against a J-Link GDB server on :2331 (see .gdbinit).
#
# A sysbuild (the mesa3w: MCUboot and jolt) loads jolt's symbols and
# MCUboot's beside them, since the two never share an address, so either can
# be stepped through, across the jump between them.  sysbuild.gdb adds `zb`
# to flash MCUboot and makes `z` flash jolt's signed image, which is what
# MCUboot boots, rather than the ELF.  The other boards build jolt alone.

if [[ -f build/jolt/zephyr/zephyr.elf ]]; then
	TERM=xterm arm-zephyr-eabi-gdb \
		-ex "add-symbol-file build/mcuboot/zephyr/zephyr.elf" \
		-x sysbuild.gdb \
		build/jolt/zephyr/zephyr.elf
else
	TERM=xterm arm-zephyr-eabi-gdb build/zephyr/zephyr.elf
fi
