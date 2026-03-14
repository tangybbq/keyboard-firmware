#include "usbd_support.h"

#include <string.h>
#include <zephyr/device.h>
#include <zephyr/sys/util.h>
#include <zephyr/usb/usbd.h>
#include <zephyr/usb/class/usbd_hid.h>

#include <zephyr/logging/log.h>
LOG_MODULE_REGISTER(jolt_usb, LOG_LEVEL_INF);

extern void usb_iface_ready_callback(bool ready);
extern void usb_input_report_done_callback(void);

static const uint8_t hid_report_desc[] = HID_KEYBOARD_REPORT_DESC();

static const struct device *hid_dev;
static struct usbd_context *usb_ctx;
static bool hid_ready;
static uint8_t last_output_report;
static uint8_t hid_protocol = 1U;

static void hid_iface_ready(const struct device *dev, const bool ready)
{
	LOG_INF("HID device %s interface is %s", dev->name, ready ? "ready" : "not ready");
	hid_ready = ready;
	usb_iface_ready_callback(ready);
}

static void hid_input_report_done(const struct device *dev,
				  const uint8_t *const report)
{
	ARG_UNUSED(dev);
	ARG_UNUSED(report);

	usb_input_report_done_callback();
}

static int hid_get_report(const struct device *dev,
			  const uint8_t type, const uint8_t id, const uint16_t len,
			  uint8_t *const buf)
{
	ARG_UNUSED(dev);
	ARG_UNUSED(id);

	if (len == 0U) {
		return 0;
	}

	if (type == HID_REPORT_TYPE_OUTPUT) {
		buf[0] = last_output_report;
		return 1;
	}

	if (type == HID_REPORT_TYPE_INPUT) {
		memset(buf, 0, MIN(len, 8));
		return MIN(len, 8);
	}

	return -ENOTSUP;
}

static int hid_set_report(const struct device *dev,
			  const uint8_t type, const uint8_t id, const uint16_t len,
			  const uint8_t *const buf)
{
	ARG_UNUSED(dev);
	ARG_UNUSED(id);

	if (type != HID_REPORT_TYPE_OUTPUT || len == 0U) {
		return -ENOTSUP;
	}

	last_output_report = buf[0];
	return 0;
}

static void hid_set_protocol(const struct device *dev, const uint8_t proto)
{
	ARG_UNUSED(dev);
	hid_protocol = proto;
	LOG_INF("HID protocol changed to %s", proto == 0U ? "boot" : "report");
}

static struct hid_device_ops hid_ops = {
	.iface_ready = hid_iface_ready,
	.get_report = hid_get_report,
	.set_report = hid_set_report,
	.set_protocol = hid_set_protocol,
	.input_report_done = hid_input_report_done,
};

static void msg_cb(struct usbd_context *const usbd_ctx,
		   const struct usbd_msg *const msg)
{
	LOG_INF("USBD message: %s", usbd_msg_type_string(msg->type));

	if (usbd_can_detect_vbus(usbd_ctx)) {
		if (msg->type == USBD_MSG_VBUS_READY) {
			if (usbd_enable(usbd_ctx)) {
				LOG_ERR("Failed to enable device support");
			}
		}

		if (msg->type == USBD_MSG_VBUS_REMOVED) {
			if (usbd_disable(usbd_ctx)) {
				LOG_ERR("Failed to disable device support");
			}
		}
	}
}

int usb_setup(void)
{
	int ret;

	hid_dev = DEVICE_DT_GET_ONE(zephyr_hid_device);
	if (!device_is_ready(hid_dev)) {
		LOG_ERR("HID device is not ready");
		return -ENODEV;
	}

	ret = hid_device_register(hid_dev, hid_report_desc, sizeof(hid_report_desc), &hid_ops);
	if (ret != 0) {
		LOG_ERR("Failed to register HID device: %d", ret);
		return ret;
	}

	usb_ctx = jolt_usbd_init_device(msg_cb);
	if (usb_ctx == NULL) {
		LOG_ERR("Failed to initialize USB device");
		return -ENODEV;
	}

	if (!usbd_can_detect_vbus(usb_ctx)) {
		ret = usbd_enable(usb_ctx);
		if (ret != 0) {
			LOG_ERR("Failed to enable USB device: %d", ret);
			return ret;
		}
	}

	return 0;
}

int usb_send_report(const uint8_t *report, uint16_t len)
{
	if (!hid_ready) {
		return -EAGAIN;
	}

	return hid_device_submit_report(hid_dev, len, report);
}
