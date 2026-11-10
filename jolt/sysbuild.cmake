# Sysbuild's part of jolt's build, for a board MCUboot boots (the mesa3w):
# one UF2 with both images, for installing over USB through the board's UF2
# bootloader instead of the J-Link.
#
# It holds MCUboot, and jolt as `z` loads it: zephyr.slot0.hex, padded to
# the whole of slot 0 so that slot 0's trailer is cleared too (images.cmake).
# It is named for the shield and the version jolt was signed with, as in
# build/mesa3w-0.1.0+12.uf2, and an older one in build/ is removed.  Nothing
# outside the internal flash is in it: the storage partition (the Bluetooth
# bonds) is past slot 0 and left as it is, and slot 1, on the QSPI flash, is
# out of the bootloader's reach.
#
# A target that always runs, and checks the inputs' times itself: the two
# hex files are made inside the images' own builds, which this build's
# ninja cannot see into.

if(SB_CONFIG_BOOTLOADER_MCUBOOT AND TARGET mcuboot)
  ExternalProject_Get_property(${DEFAULT_IMAGE} BINARY_DIR)
  set(jolt_dir ${BINARY_DIR})
  ExternalProject_Get_property(mcuboot BINARY_DIR)
  set(mcuboot_dir ${BINARY_DIR})

  if(${DEFAULT_IMAGE}_SHIELD)
    set(uf2_name ${${DEFAULT_IMAGE}_SHIELD})
  else()
    set(uf2_name ${DEFAULT_IMAGE})
  endif()

  add_custom_target(combined_uf2 ALL
    COMMAND ${CMAKE_COMMAND}
      -DPYTHON=${PYTHON_EXECUTABLE}
      -DZEPHYR_BASE=${ZEPHYR_BASE}
      -DMCUBOOT_HEX=${mcuboot_dir}/zephyr/zephyr.hex
      -DSLOT0_HEX=${jolt_dir}/zephyr/zephyr.slot0.hex
      -DVERSION_FILE=${jolt_dir}/zephyr/zephyr.version
      -DOUT_DIR=${CMAKE_BINARY_DIR}
      -DNAME=${uf2_name}
      -P ${APP_DIR}/cmake/combined-uf2.cmake
    VERBATIM
  )
  add_dependencies(combined_uf2 ${DEFAULT_IMAGE} mcuboot)
endif()
