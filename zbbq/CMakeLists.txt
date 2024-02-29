cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(zbbq)

FILE(GLOB app_sources src/*.c)
target_sources(app PRIVATE ${app_sources})

set(BUILD_LIB_DIR ${CMAKE_CURRENT_SOURCE_DIR}/thumbv6m-none-eabi/debug)

set(CARGO_TARGET_DIR ${CMAKE_CURRENT_BINARY_DIR}/rust/target-kbbq)
set(RUST_LIBRARY ${CARGO_TARGET_DIR}/thumbv6m-none-eabi/debug/librust_kbbq.a)

# Let's see if chatGPT can give me enough to make this work.
add_custom_target(
    librust_kbbq ALL
    # OUTPUT ${RUST_LIBRARY}
    COMMAND cargo build
        # --release
        --target thumbv6m-none-eabi
        --target-dir ${CARGO_TARGET_DIR}
    COMMENT "Building Rust application"
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)

# This custom target should then always trigger the custom command (I have
# doubts)
# add_custom_target(librust_kbbq ALL
#    DEPENDS ${RUST_LIBRARY}
# )

# And all of the above still won't relink the application if the rust code is
# the only thing that has changed. To force this, we have to do lots of extra
# stupidity.
set(DUMMY_FILE "${CMAKE_BINARY_DIR}/always-relink.dummy")
add_custom_command(
    OUTPUT ${DUMMY_FILE}
    COMMENT "Touching dummy file.  Should always happen."
    COMMAND touch ${DUMMY_FILE}-not
)
add_custom_target(ForceRelink ALL DEPENDS ${DUMMY_FILE})

# Link
target_link_libraries(app PRIVATE ${RUST_LIBRARY})

# Make sure this is always built.
add_dependencies(app ForceRelink librust_kbbq)
set_target_properties(app PROPERTIES
    ADDITIONAL_CLEAN_FILES ${DUMMY_FILE}
    OBJECT_DEPENDS ${RUST_LIBRARY}
)
