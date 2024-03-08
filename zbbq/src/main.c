// C Main for zbbq firmware.
//
// This code is primarily written in Rust, but as we're just beginning with Rust
// bindings, there will be various pieces of code in this file to glue them
// together, including invoking the `rust_main` entry.

#include "zephyr/drivers/usb/usb_dc.h"
#include "zephyr/usb/class/hid.h"
#include <zephyr/kernel.h>
#include <stdlib.h>

#include <zephyr/usb/usb_device.h>
#include <zephyr/usb/class/usb_hid.h>

#define LOG_LEVEL 4
#include <zephyr/logging/log.h>
LOG_MODULE_REGISTER(zbbq);

extern void rust_main(void);

K_SEM_DEFINE(usb_sem, 1, 1);
const struct device *hid0_dev;

// Callbacks from USB.
static void in_ready_cb(const struct device *dev)
{
	// LOG_INF("HID in_ready");
	k_sem_give(&usb_sem);
}

// TO RUST: Indicates if the USB-HID is accepting of a keypress.
int is_hid_accepting(void) {
	return k_sem_count_get(&usb_sem) > 0;
}

// TO RUST: Send the given report (8 bytes) to the hid interface.  Assumes that
// `is_hid_accepting` is available (will stall if not).
void hid_report(uint8_t *report) {
	k_sem_take(&usb_sem, K_FOREVER);
	hid_int_ep_write(hid0_dev, report, 8, NULL);
}

static const struct hid_ops ops = {
	.int_in_ready = in_ready_cb,
};

// Use a basic keyboard HID report for boot mode.  As long as we aren't doing
// NKRO, this should be adequate.
static const uint8_t hid_kbd_report_desc[] = HID_KEYBOARD_REPORT_DESC();

static void status_cb(enum usb_dc_status_code status, const uint8_t *param)
{
	LOG_INF("USB status: %d", status);
}

#define DEVICE_AND_COMMA(node_id) DEVICE_DT_GET(node_id),

int main(void) {
	const struct device *cdc_dev[] = {
		DT_FOREACH_STATUS_OKAY(zephyr_cdc_acm_uart, DEVICE_AND_COMMA)
	};
	hid0_dev = device_get_binding("HID_0");
	if (hid0_dev == NULL) {
		LOG_ERR("Cannotr get USB HID 0 Device");
		return 0;
	}

	for (int idx = 0; idx < ARRAY_SIZE(cdc_dev); idx++) {
		if (!device_is_ready(cdc_dev[idx])) {
			LOG_ERR("CDC ADM DEVICE %s is not ready",
				cdc_dev[idx]->name);
			return 0;
		}
	}

	usb_hid_register_device(hid0_dev, hid_kbd_report_desc,
				sizeof(hid_kbd_report_desc), &ops);
	usb_hid_init(hid0_dev);

	int ret = usb_enable(status_cb);
	if (ret != 0) {
		LOG_ERR("Failed to enable USB");
		return 0;
	}

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
