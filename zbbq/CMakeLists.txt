cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(zbbq)

FILE(GLOB app_sources src/*.c)
target_sources(app PRIVATE ${app_sources})

set(BUILD_LIB_DIR ${CMAKE_CURRENT_SOURCE_DIR}/thumbv6m-none-eabi/debug)

set(CARGO_TARGET_DIR ${CMAKE_CURRENT_BINARY_DIR}/rust/target-kbbq)
set(RUST_LIBRARY ${CARGO_TARGET_DIR}/thumbv6m-none-eabi/release/libkbbq.a)

# Getting CMake to always run Cargo is a bit of a mess. The trick is to make the
# custom command's output file be a file that never gets created. This will
# cause this to always rerun the cargo command. We add the actual library as a
# BYPRODUCTS list of this command, otherwise, the first time, the link will fail
# because it doesn't think it knows how to build the library. This will also
# cause the relink when the cargo command actually does rebuild the rust code.

set(DUMMY_FILE "${CMAKE_BINARY_DIR}/always-run-cargo.dummy")

# The library can be built by just invoking Cargo
add_custom_command(
  OUTPUT ${DUMMY_FILE}
  BYPRODUCTS ${RUST_LIBRARY}
  COMMAND cargo build
    --release
    --target thumbv6m-none-eabi
    --target-dir ${CARGO_TARGET_DIR}
  COMMENT "Building Rust application"
  WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)

add_custom_target(libkbbq ALL
  DEPENDS ${DUMMY_FILE}
)

# Link
target_link_libraries(app PRIVATE ${RUST_LIBRARY})

# Make sure this is always built.
add_dependencies(app libkbbq)
