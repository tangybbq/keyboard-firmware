# GDB commands for a sysbuild of MCUboot and jolt (the mesa3w), sourced by
# gdb.sh after .gdbinit.  The images are b-mesa3w.sh's.

# Flash jolt: the image signed for, and padded to, the whole of slot 0, so
# that slot 0's trailer is rewritten too.  The ELF alone has no MCUboot
# header, and MCUboot would not boot it.
define z
  load build/jolt/zephyr/zephyr.slot0.hex
  monitor reset
  maintenance flush register-cache
end

# Flash MCUboot.  It verifies nothing in slot 0 (BOOT_VALIDATE_SLOT0=n), so
# a slot 0 signed with an older key keeps booting until the next update.
define zb
  load build/mcuboot/zephyr/zephyr.elf
  monitor reset
  maintenance flush register-cache
end

# Stop in MCUboot, just before it jumps to jolt.
define bjump
  b do_boot
end
