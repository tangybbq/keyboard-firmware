// C Main for zbbq firmware.
//
// This code is primarily written in Rust, but as we're just beginning with Rust
// bindings, there will be various pieces of code in this file to glue them
// together, including invoking the `rust_main` entry.

#include "zephyr/kernel/thread_stack.h"
#include <zephyr/kernel.h>
#include <stdlib.h>

#define LOG_LEVEL 4
#include <zephyr/logging/log.h>
LOG_MODULE_REGISTER(zbbq);

extern void rust_main(void);
extern int usb_setup(void);

extern void steno_thread_main(void *p1, void *p2, void *p3);
extern void init_queues(void);

#define STENO_THREAD_STACK_SIZE 8192

K_THREAD_STACK_DEFINE(steno_thread_stack, 8192);
struct k_thread steno_thread;

int main(void) {
	// Initialize the queues used to communicate.
	init_queues();

	int ret = usb_setup();
	if (ret != 0) {
		return ret;
	}

	// Start the lower priority steno thread, which will do the dictionary
	// lookups.
	(void) k_thread_create(&steno_thread,
			       steno_thread_stack,
			       STENO_THREAD_STACK_SIZE,
			       steno_thread_main,
			       0, 0, 0,
			       10, 0, K_NO_WAIT);

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

K_TIMER_DEFINE(heartbeat_timer, NULL, NULL);
K_MUTEX_DEFINE(event_queue_mutex);
K_CONDVAR_DEFINE(event_queue_condvar);
K_MUTEX_DEFINE(steno_queue_mutex);
K_CONDVAR_DEFINE(steno_queue_condvar);
