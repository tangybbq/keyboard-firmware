# zephyr.slot0.hex: jolt signed for the J-Link, beside the build's own
# signed images.  Included from CMakeLists.txt when MCUboot boots jolt (the
# mesa3w); gdb.sh's `z` loads it.
#
# The build signs jolt for slot 1, where an update goes: its padded images
# end at slot 1's size, a sector short of slot 0's end, where MCUboot keeps
# slot 0's trailer.  This one is padded to the whole of slot 0 and marked
# permanent, so loading it rewrites slot 0's trailer too, and one left by an
# earlier update, which could have MCUboot revert to whatever is in slot 1,
# never survives.  The other arguments are the build's own.
#
# imgtool's hex output leaves the padding out, so the J-Link would rewrite
# only the sectors with code in them and the last one, and the trailer is
# two sectors long (256 sectors of swap status).  Signing to a bin and
# converting that keeps every 0xff, and the J-Link erases the whole slot.
#
# A custom command rather than a step in b-mesa3w.sh, so that an ordinary
# incremental `west build` brings it up to date with the rest.

dt_nodelabel(slot0_node NODELABEL "slot0_partition" REQUIRED)
dt_reg_addr(slot0_addr PATH ${slot0_node})
dt_reg_size(slot0_size PATH ${slot0_node})
dt_chosen(flash_node PROPERTY "zephyr,flash")
dt_prop(write_block_size PATH ${flash_node} PROPERTY "write-block-size")

set(kernel_dir ${APPLICATION_BINARY_DIR}/zephyr)
set(slot0_bin ${kernel_dir}/${KERNEL_NAME}.slot0.bin)
set(slot0_hex ${kernel_dir}/${KERNEL_NAME}.slot0.hex)

add_custom_command(
  OUTPUT ${slot0_hex}
  BYPRODUCTS ${slot0_bin}
  COMMAND ${PYTHON_EXECUTABLE} ${IMGTOOL} sign
    --version ${CONFIG_MCUBOOT_IMGTOOL_SIGN_VERSION}
    --header-size ${CONFIG_ROM_START_OFFSET}
    --slot-size ${slot0_size}
    --align ${write_block_size}
    --key ${CONFIG_MCUBOOT_SIGNATURE_KEY_FILE}
    --pad --confirm
    ${kernel_dir}/${KERNEL_BIN_NAME} ${slot0_bin}
  COMMAND ${CMAKE_OBJCOPY} -I binary -O ihex --change-addresses ${slot0_addr}
    ${slot0_bin} ${slot0_hex}
  DEPENDS ${logical_target_for_zephyr_elf} ${kernel_dir}/${KERNEL_BIN_NAME}
    ${CONFIG_MCUBOOT_SIGNATURE_KEY_FILE}
  COMMENT "Signing jolt for slot 0: ${KERNEL_NAME}.slot0.hex"
  VERBATIM
)
add_custom_target(slot0_image ALL DEPENDS ${slot0_hex})
