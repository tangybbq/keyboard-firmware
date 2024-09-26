// Heartbeat timer.

#include <zephyr/kernel.h>

struct k_timer heartbeat_timer;

extern void rust_heartbeat(void);

static void hb_tick(struct k_timer *timer) {
	rust_heartbeat();
}

void setup_heartbeat(void) {
	k_timer_init(&heartbeat_timer, hb_tick, NULL);
	k_timer_start(&heartbeat_timer, K_MSEC(1), K_MSEC(1));
}
