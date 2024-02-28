/* My example */

#include <zephyr/kernel.h>
#include <zephyr/logging/log_ctrl.h>

#define LOG_LEVEL 4
#include <zephyr/logging/log.h>
LOG_MODULE_REGISTER(main);

static void wait_on_log_flushed(void)
{
	// Disable this.
	if (1) {
		return;
	}
	while (log_buffered_cnt()) {
		k_sleep(K_MSEC(5));
	}
}

extern void rust_main(void);

// Simple logging.
void msg(const char* msg)
{
	LOG_INF("rust: %s", msg);
}

int main(void)
{
	LOG_INF("Hello world, from C main");
	wait_on_log_flushed();

	rust_main();

	// Log a periodic message.
	int count = 0;
	while (1) {
		count++;
		LOG_INF("Tick: %d", count);
		wait_on_log_flushed();
		k_sleep(K_MSEC(1000));
	}
}
