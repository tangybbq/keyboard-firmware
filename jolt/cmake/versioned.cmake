# Run a command with the version bump-build.cmake wrote in place of every
# argument that is just @VERSION@.
#
#   cmake -DVERSION_FILE=<file> -P versioned.cmake -- <command> <args>...

file(STRINGS "${VERSION_FILE}" version LIMIT_COUNT 1)
string(STRIP "${version}" version)

set(command)
set(started FALSE)
math(EXPR last "${CMAKE_ARGC} - 1")
foreach(i RANGE ${last})
  set(arg "${CMAKE_ARGV${i}}")
  if(started)
    if(arg STREQUAL "@VERSION@")
      set(arg "${version}")
    endif()
    list(APPEND command "${arg}")
  elseif(arg STREQUAL "--")
    set(started TRUE)
  endif()
endforeach()
if(NOT command)
  message(FATAL_ERROR "versioned.cmake: no command after --")
endif()

execute_process(COMMAND ${command} RESULT_VARIABLE result)
if(NOT result EQUAL 0)
  message(FATAL_ERROR "versioned.cmake: ${command} failed: ${result}")
endif()
