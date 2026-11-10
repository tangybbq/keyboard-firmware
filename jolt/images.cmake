# jolt's MCUboot images, beside the build's own signed ones.  Included from
# CMakeLists.txt when MCUboot boots jolt (the mesa3w).
#
# zephyr.slot0.hex: jolt signed for the J-Link; gdb.sh's `z` loads it.
# zephyr.update.bin: jolt signed for an update; `keyminder upgrade` uploads
# it.
#
# The build signs jolt for slot 1, where an update goes: its padded images
# end at slot 1's size, a sector short of slot 0's end, where MCUboot keeps
# slot 0's trailer.  This one is padded to the whole of slot 0 and marked
# permanent, so loading it rewrites slot 0's trailer too, and one left by an
# earlier update, which could have MCUboot revert to whatever is in slot 1,
# never survives.  The other arguments, but for the version (below), are
# the build's own.
#
# imgtool's hex output leaves the padding out, so the J-Link would rewrite
# only the sectors with code in them and the last one, and the trailer is
# two sectors long (256 sectors of swap status).  Signing to a bin and
# converting that keeps every 0xff, and the J-Link erases the whole slot.
#
# The update is padded to the whole of slot 1, with its trailer marked
# pending on trial (imgtool's --pad without --confirm), so that writing it
# into slot 1 page for page is the whole of an upload: MCUboot swaps it in at
# the next reset, and jolt confirms it once a host connects (image.c).  The
# build's own zephyr.signed.bin has no trailer, and its confirmed image is
# permanent, which would leave nothing to revert to.
#
# Custom commands rather than steps in b-mesa3w.sh, so that an ordinary
# incremental `west build` brings them up to date with the rest.
#
# Both are signed with the same version: major.minor.patch from jolt's
# VERSION file, set by hand, and a build number counted in `build-number`
# (untracked; see .gitignore), one more each time zephyr.bin is linked.  It
# lives beside VERSION rather than in the build directory so that a clean
# build (b-mesa3w.sh) does not start it again from 1.  jolt reads the
# version back from its image's header (image.c), so a new number needs no
# recompiling.  Zephyr's own signed images keep VERSION's build number, 0:
# Zephyr reads it only at configure time, and allows no more than 255.

dt_nodelabel(slot0_node NODELABEL "slot0_partition" REQUIRED)
dt_reg_addr(slot0_addr PATH ${slot0_node})
dt_reg_size(slot0_size PATH ${slot0_node})
dt_chosen(flash_node PROPERTY "zephyr,flash")
dt_prop(write_block_size PATH ${flash_node} PROPERTY "write-block-size")

set(kernel_dir ${APPLICATION_BINARY_DIR}/zephyr)
set(slot0_bin ${kernel_dir}/${KERNEL_NAME}.slot0.bin)
set(slot0_hex ${kernel_dir}/${KERNEL_NAME}.slot0.hex)

set(version_file ${kernel_dir}/${KERNEL_NAME}.version)
add_custom_command(
  OUTPUT ${version_file}
  COMMAND ${CMAKE_COMMAND}
    -DCOUNTER=${APPLICATION_SOURCE_DIR}/build-number
    -DRELEASE=${APP_VERSION_MAJOR}.${APP_VERSION_MINOR}.${APP_PATCHLEVEL}
    -DOUT=${version_file}
    -P ${APPLICATION_SOURCE_DIR}/cmake/bump-build.cmake
  DEPENDS ${logical_target_for_zephyr_elf} ${kernel_dir}/${KERNEL_BIN_NAME}
    ${APPLICATION_SOURCE_DIR}/cmake/bump-build.cmake
  COMMENT "Counting a new build of jolt"
  VERBATIM
)

# imgtool, with the version the build was counted as.
set(imgtool_versioned
  ${CMAKE_COMMAND} -DVERSION_FILE=${version_file}
    -P ${APPLICATION_SOURCE_DIR}/cmake/versioned.cmake --
    ${PYTHON_EXECUTABLE} ${IMGTOOL}
)

add_custom_command(
  OUTPUT ${slot0_hex}
  BYPRODUCTS ${slot0_bin}
  COMMAND ${imgtool_versioned} sign
    --version @VERSION@
    --header-size ${CONFIG_ROM_START_OFFSET}
    --slot-size ${slot0_size}
    --align ${write_block_size}
    --key ${CONFIG_MCUBOOT_SIGNATURE_KEY_FILE}
    --pad --confirm
    ${kernel_dir}/${KERNEL_BIN_NAME} ${slot0_bin}
  COMMAND ${CMAKE_OBJCOPY} -I binary -O ihex --change-addresses ${slot0_addr}
    ${slot0_bin} ${slot0_hex}
  DEPENDS ${logical_target_for_zephyr_elf} ${kernel_dir}/${KERNEL_BIN_NAME}
    ${CONFIG_MCUBOOT_SIGNATURE_KEY_FILE} ${version_file}
    ${APPLICATION_SOURCE_DIR}/cmake/versioned.cmake
  COMMENT "Signing jolt for slot 0: ${KERNEL_NAME}.slot0.hex"
  VERBATIM
)
add_custom_target(slot0_image ALL DEPENDS ${slot0_hex})

dt_nodelabel(slot1_node NODELABEL "slot1_partition" REQUIRED)
dt_reg_size(slot1_size PATH ${slot1_node})
set(update_bin ${kernel_dir}/${KERNEL_NAME}.update.bin)

add_custom_command(
  OUTPUT ${update_bin}
  COMMAND ${imgtool_versioned} sign
    --version @VERSION@
    --header-size ${CONFIG_ROM_START_OFFSET}
    --slot-size ${slot1_size}
    --align ${write_block_size}
    --key ${CONFIG_MCUBOOT_SIGNATURE_KEY_FILE}
    --pad
    ${kernel_dir}/${KERNEL_BIN_NAME} ${update_bin}
  DEPENDS ${logical_target_for_zephyr_elf} ${kernel_dir}/${KERNEL_BIN_NAME}
    ${CONFIG_MCUBOOT_SIGNATURE_KEY_FILE} ${version_file}
    ${APPLICATION_SOURCE_DIR}/cmake/versioned.cmake
  COMMENT "Signing jolt for an update: ${KERNEL_NAME}.update.bin"
  VERBATIM
)
add_custom_target(update_image ALL DEPENDS ${update_bin})
