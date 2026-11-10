# Count a new build of jolt: add one to the build counter, and write the
# version to sign it with.  Run by images.cmake each time zephyr.bin is
# linked.
#
#   cmake -DCOUNTER=<file> -DRELEASE=<major.minor.patch> -DOUT=<file> -P bump-build.cmake
#
# COUNTER holds the last build's number, and is created at 0 if missing.
# OUT gets RELEASE+number, the form imgtool's --version takes; MCUboot keeps
# the number as the image header's 32-bit build number.

set(number 0)
if(EXISTS "${COUNTER}")
  file(STRINGS "${COUNTER}" lines LIMIT_COUNT 1)
  string(STRIP "${lines}" number)
  if(NOT number MATCHES "^[0-9]+$")
    message(FATAL_ERROR "${COUNTER} should hold a build number, not \"${number}\"")
  endif()
endif()
math(EXPR number "${number} + 1")
if(number GREATER 4294967295)
  message(FATAL_ERROR "${COUNTER} has run past MCUboot's 32-bit build number")
endif()

file(WRITE "${COUNTER}" "${number}\n")
file(WRITE "${OUT}" "${RELEASE}+${number}\n")
message(STATUS "jolt build ${RELEASE}+${number}")
