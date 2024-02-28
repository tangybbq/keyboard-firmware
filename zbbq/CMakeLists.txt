cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(zbbq)

FILE(GLOB app_sources src/*.c)
target_sources(app PRIVATE ${app_sources})

set(BUILD_LIB_DIR ${CMAKE_CURRENT_SOURCE_DIR}/thumbv6m-none-eabi/debug)

set(CARGO_TARGET_DIR ${CMAKE_CURRENT_BINARY_DIR}/rust/target-kbbq)
set(RUST_LIBRARY ${CARGO_TARGET_DIR}/thumbv6m-none-eabi/debug/librust_kbbq.a)

### Let's see if we can figure out some incantation to get this pile of shit
### known as CMake to actually invoke cargo every time.

# Let's see if chatGPT can give me enough to make this work.
add_custom_command(
    OUTPUT ${RUST_LIBRARY}
    COMMAND cargo build
        # --release
        --target thumbv6m-none-eabi
        --target-dir ${CARGO_TARGET_DIR}
    COMMENT "Building Rust application"
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)

# This custom target should then always trigger the custom command (I have
# doubts)
add_custom_target(librust_kbbq ALL
    DEPENDS ${RUST_LIBRARY}
)

# Make sure this is always built.
add_dependencies(app librust_kbbq)

# Link
target_link_libraries(app PRIVATE ${RUST_LIBRARY})
