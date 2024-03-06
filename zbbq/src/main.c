// C Main for zbbq firmware.
//
// This code is primarily written in Rust, but as we're just beginning with Rust
// bindings, there will be various pieces of code in this file to glue them
// together, including invoking the `rust_main` entry.

#include <zephyr/kernel.h>
#include <stdlib.h>

#define LOG_LEVEL 4
#include <zephyr/logging/log.h>
LOG_MODULE_REGISTER(zbbq);

extern void rust_main(void);

int main(void) {
	rust_main();
}
