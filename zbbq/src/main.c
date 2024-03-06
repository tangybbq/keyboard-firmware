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

/// Panic coming from the Rust side.
/// TODO: Pass in information about the context.
void c_k_panic() {
	k_panic();
}

/// Log a message from the Rust side.
void c_log_message(int level, const char *text) {
	// The log levels in Zephyr aren't numbers, although they are.
	switch (level) {
        case LOG_LEVEL_ERR:
		LOG_ERR("%s", text);
		break;
        case LOG_LEVEL_WRN:
		LOG_WRN("%s", text);
		break;
        case LOG_LEVEL_INF:
		LOG_INF("%s", text);
		break;
        case LOG_LEVEL_DBG:
	default:
		LOG_DBG("%s", text);
		break;
	}
}
