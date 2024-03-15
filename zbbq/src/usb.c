// C Main for zbbq firmware.
//
// This code is primarily written in Rust, but as we're just beginning with Rust
// bindings, there will be various pieces of code in this file to glue them
// together, including invoking the `rust_main` entry.

#include "zephyr/drivers/usb/usb_dc.h"
#include "zephyr/usb/class/hid.h"
#include <zephyr/kernel.h>

#include <zephyr/usb/usb_device.h>
#include <zephyr/usb/class/usb_hid.h>

#include <zephyr/drivers/uart.h>

#include <zephyr/logging/log.h>
LOG_MODULE_DECLARE(zbbq);

K_SEM_DEFINE(usb_sem, 1, 1);
const struct device *hid0_dev;

#define DEVICE_AND_COMMA(node_id) DEVICE_DT_GET(node_id),
const struct device *cdc_dev[] = {
		DT_FOREACH_STATUS_OKAY(zephyr_cdc_acm_uart, DEVICE_AND_COMMA)
};
#undef DEVICE_AND_COMMA

// Check that we aren't in an isr context.  To quickly catch problems.
#define NO_ISR()                                                               \
  do {                                                                         \
    if (k_is_in_isr()) {                                                       \
      k_panic();                                                               \
    }                                                                          \
  } while (0)


// Callbacks from USB.
static void in_ready_cb(const struct device *dev)
{
	NO_ISR();
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

extern void rust_usb_status(uint32_t);

static void status_cb(enum usb_dc_status_code status, const uint8_t *param)
{
	NO_ISR();
	// Currently, we only care about configured and suspend, which we will
	// pass to the Rust code.
	switch (status) {
	case USB_DC_CONFIGURED:
		rust_usb_status(0);
		break;
	case USB_DC_SUSPEND:
		rust_usb_status(1);
		break;
	case USB_DC_RESUME:
		rust_usb_status(2);
	default:
		break;
	}
	LOG_INF("USB status: %d", status);
}

void acm_write(int index, const uint8_t *buf, int len)
{
	const struct device *dev = cdc_dev[index];

	uint32_t ctrl;
	int res = uart_line_ctrl_get(dev, UART_LINE_CTRL_DTR, &ctrl);
	// LOG_INF("uart dtr: %d, res=%d", ctrl, res);

	// If DTS is not asserted, it means that nobody is attached to the ACM
	// endpoint.  Don't try sending, or keys will queue up.
	if (res != 0 || !ctrl) {
		return;
	}

	uart_irq_tx_enable(dev);
	while (len) {
		int written = uart_fifo_fill(dev, buf, len);
		if (written != len) {
			LOG_INF("Not written: %d", written);
		}
		break;
	}

	uart_irq_tx_disable(dev);
}

// Indicates if the given ACM device has DTR asserted.
int acm_has_dtr(int index)
{
	const struct device *dev = cdc_dev[index];

	uint32_t ctrl;
	int res = uart_line_ctrl_get(dev, UART_LINE_CTRL_DTR, &ctrl);

	return res == 0 && ctrl;
}

int usb_setup(void) {
	hid0_dev = device_get_binding("HID_0");
	if (hid0_dev == NULL) {
		LOG_ERR("Cannot get USB HID 0 Device");
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

	return 0;
}
