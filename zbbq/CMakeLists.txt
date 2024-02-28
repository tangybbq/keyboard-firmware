cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(zbbq)

FILE(GLOB app_sources src/*.c)
target_sources(app PRIVATE ${app_sources})

set(BUILD_LIB_DIR ${CMAKE_CURRENT_SOURCE_DIR}/thumbv6m-none-eabi/debug)

set(CARGO_TARGET_DIR ${CMAKE_CURRENT_BINARY_DIR}/rust/target-kbbq)
set(RUST_LIBRARY ${CARGO_TARGET_DIR}/thumbv6m-none-eabi/debug/librust_kbbq.a)

add_custom_command(
  OUTPUT ${RUST_LIBRARY}
  COMMAND cargo build
          # --release
          --target thumbv6m-none-eabi
          --target-dir ${CARGO_TARGET_DIR}
  WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)

add_custom_target(
  librust_kbbq ALL
  DEPENDS ${RUST_LIBRARY}
)

add_dependencies(app librust_kbbq)
target_link_libraries(app PUBLIC ${RUST_LIBRARY})
