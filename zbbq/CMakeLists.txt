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

message(STATUS "RS directory: ${ZEPHYR_RS_MODULE_DIR}")
if(NOT DEFINED ZEPHYR_RS_MOUDLE_DIR)
  # TODO: Why does this check not work?
  # message(FATAL_ERROR "Unable to find zephyr-rs module")
endif()

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
set(SAMPLE_CARGO_CONFIG ${CMAKE_CURRENT_BINARY_DIR}/rust/sample-cargo-config.toml)

# The shield selected distinguishes the 2-row and 3-row keyboards. Note that the
# only way to specify features for the top level build is through the command
# line. As such, the packages may not be built with the correct features if
# cargo is invoked directly. Cmake should rerun the cargo build with the right
# arguments, though, so builds should be correct. The default feature can be set
# in Cargo.toml.
if(NOT DEFINED SHIELD)
  message(FATAL_ERROR "Must select a shield to build the keyboard firmware.")
endif()
if(SHIELD STREQUAL "proto2")
  set(SHIELD_FEATURE "proto2")
elseif(SHIELD STREQUAL "proto3")
  set(SHIELD_FEATURE "proto3")
elseif(SHIELD STREQUAL "proto4")
  set(SHIELD_FEATURE "proto3")
else()
  message(FATAL_ERROR "SHIELD is unexpected value: ${SHIELD}")
endif()

# Getting CMake to always run Cargo is a bit of a mess. The trick is to make the
# custom command's output file be a file that never gets created. This will
# cause this to always rerun the cargo command. We add the actual library as a
# BYPRODUCTS list of this command, otherwise, the first time, the link will fail
# because it doesn't think it knows how to build the library. This will also
# cause the relink when the cargo command actually does rebuild the rust code.

set(DUMMY_FILE "${CMAKE_BINARY_DIR}/always-run-cargo.dummy")

# Write out a cargo config file that can be copied into `.cargo/config.toml` in
# the source directory to allow various IDE tools and such to work. The build we
# invoke will override these settings, in case they are out of date. Everything
# set here should match the arguments given to the cargo build command below.
file(WRITE ${SAMPLE_CARGO_CONFIG} "
# This is a generated sample .cargo/config.toml file from the Zephyr file
# At the time of generation, this represented the settings needed to allow
# a `cargo build` to compile the rust code using the current Zephyr build.
# If any settings in the Zephyr build change, this could become out of date.
[build]
target = \"thumbv6m-none-eabi\"
target-dir = \"${CARGO_TARGET_DIR}\"

[env]
BUILD_DIR = \"${CMAKE_CURRENT_BINARY_DIR}\"
DOTCONFIG = \"${DOTCONFIG}\"

[patch.crates-io]
zephyr-sys.path = \"${ZEPHYR_RS_MODULE_DIR}/zephyr-sys\"
")

# The library can be built by just invoking Cargo
add_custom_command(
  OUTPUT ${DUMMY_FILE}
  BYPRODUCTS ${RUST_LIBRARY}
  COMMAND
    ${CMAKE_EXECUTABLE}
    env BUILD_DIR=${CMAKE_CURRENT_BINARY_DIR}
    DOTCONFIG=${DOTCONFIG}
    cargo build
    # TODO: release flag if release build
    # --release

    # Override the features according to the shield given. For a general case,
    # this will need to come from a variable or argument.
    --no-default-features
    --features ${SHIELD_FEATURE}

    # Set a replacement so that packages can just use `zephyr-sys` as a package
    # name to find it.
    --config "patch.crates-io.zephyr-sys.path=\\\"${ZEPHYR_RS_MODULE_DIR}/zephyr-sys\\\""
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
