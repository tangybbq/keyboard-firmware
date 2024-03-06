cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(zbbq)

FILE(GLOB app_sources src/*.c)
target_sources(app PRIVATE ${app_sources})

# Build stragety for a Rust 'main' application. There are a few constraints that
# cargo places on us that require a bit of trickery to get around. First,
# anything that can affect the compilation, such as config values need to be
# given to the invocation of cargo. This can be done either through the command
# line, or through files such as `.cargo/config.toml`, which is first searched
# for in the source directory. Because there are potential limits on the length
# of the command line (as well as the environment), we'll need to use the
# `.cargo/config.toml`. To make this work, we will create a fresh source
# directory in the build directory, and populate it with some of the files
# necessary to build. The `Cargo.toml` file will come from the user's app
# directory. The source files will be populated to match those in the source
# directory, using the include compiler directive within Rust. This tree will
# mirror the source tree. Then the `.cargo/config.toml` will be configured for
# this particular build. Specifically, the target directory, the target itself,
# and a mapping of Zephyr Kconfig values onto config features to rustc.

# TODO: Make sure RUSTFLAGS is not set.
# TODO: Create source directory.
# TODO: Copy source Cargo.toml
# TODO: Mirror source tree
# TODO: Synthesize .cargo/config.toml
# TODO: Convert Kconfig entries from Zephyr into Rust configs.

set(RUST_BUILD_TYPE debug)

set(BUILD_LIB_DIR ${CMAKE_CURRENT_SOURCE_DIR}/thumbv6m-none-eabi/debug)

set(CARGO_TARGET_DIR ${CMAKE_CURRENT_BINARY_DIR}/rust/target-kbbq)
set(RUST_LIBRARY ${CARGO_TARGET_DIR}/thumbv6m-none-eabi/${RUST_BUILD_TYPE}/libkbbq.a)

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
    # TODO: release flag if release build
    # --release
    --target thumbv6m-none-eabi
    --target-dir ${CARGO_TARGET_DIR}
  COMMENT "Building Rust application"
  WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)

add_custom_target(libkbbq ALL
  DEPENDS ${DUMMY_FILE}
)

# Link. The allow multiple definition avoids at least fatal link errors with
# definitions coming from both libgcc.a and the application built by Rust.
# See: https://github.com/rust-lang/compiler-builtins/issues/345
target_link_libraries(app PUBLIC -Wl,--allow-multiple-definition ${RUST_LIBRARY})

# Make sure this is always built.
add_dependencies(app libkbbq)
