# Merge MCUboot and jolt's slot 0 image into one UF2, named for the version
# jolt was signed with.  Run by sysbuild.cmake.
#
#   cmake -DPYTHON=<python> -DZEPHYR_BASE=<dir> -DMCUBOOT_HEX=<hex>
#         -DSLOT0_HEX=<hex> -DVERSION_FILE=<file> -DOUT_DIR=<dir> -DNAME=<name>
#         -P combined-uf2.cmake
#
# Writes OUT_DIR/NAME-<version>.uf2, unless it is already newer than both
# hex files, and removes any other NAME-*.uf2 there.

# The family the Adafruit nRF52 bootloader, the XIAO's, takes for an
# nRF52840 application.
set(family 0xADA52840)

file(STRINGS "${VERSION_FILE}" version LIMIT_COUNT 1)
string(STRIP "${version}" version)
set(uf2 "${OUT_DIR}/${NAME}-${version}.uf2")

if(EXISTS "${uf2}"
    AND NOT "${MCUBOOT_HEX}" IS_NEWER_THAN "${uf2}"
    AND NOT "${SLOT0_HEX}" IS_NEWER_THAN "${uf2}")
  return()
endif()

file(GLOB old "${OUT_DIR}/${NAME}-*.uf2")
if(old)
  file(REMOVE ${old})
endif()

set(merged "${OUT_DIR}/${NAME}.hex")
execute_process(
  COMMAND ${PYTHON} ${ZEPHYR_BASE}/scripts/build/mergehex.py
    -o ${merged} ${MCUBOOT_HEX} ${SLOT0_HEX}
  RESULT_VARIABLE result
)
if(NOT result EQUAL 0)
  message(FATAL_ERROR "combined-uf2.cmake: merging the hex files failed")
endif()

execute_process(
  COMMAND ${PYTHON} ${ZEPHYR_BASE}/scripts/build/uf2conv.py
    -c -f ${family} -o ${uf2} ${merged}
  OUTPUT_QUIET
  RESULT_VARIABLE result
)
if(NOT result EQUAL 0)
  file(REMOVE "${uf2}")
  message(FATAL_ERROR "combined-uf2.cmake: converting to UF2 failed")
endif()
file(REMOVE "${merged}")
message(STATUS "MCUboot and jolt ${version} for USB: ${uf2}")
