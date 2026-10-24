/*
 * The minder interface: a vendor-specific USB function with one bulk OUT and
 * one bulk IN endpoint, 64 bytes each, as jolt-embassy-rp's usb.rs adds.  It
 * carries the minder protocol: CBOR messages, each ended by a short packet (a
 * zero-length one when the message is a multiple of 64 bytes).
 *
 * Whole messages, not packets, cross to Rust.  An OUT transfer into a buffer
 * bigger than any request finishes at the request's short packet, so Rust is
 * handed one complete request at a time; Rust hands over a complete reply,
 * which goes out as one transfer, with the zero-length packet the framing
 * needs.  One of each can be in flight.
 *
 * The new USB stack has no generic vendor class, so this is one, after
 * Zephyr's samples/subsys/usb/webusb/src/sfunc.c.
 */

#include <errno.h>
#include <string.h>
#include <zephyr/drivers/usb/udc.h>
#include <zephyr/logging/log.h>
#include <zephyr/sys/atomic.h>
#include <zephyr/sys/byteorder.h>
#include <zephyr/usb/usbd.h>

LOG_MODULE_REGISTER(jolt_minder, LOG_LEVEL_INF);

/*
 * Bigger than the biggest message either way: jolt's minder accepts requests
 * up to 4200 bytes (a 4 KiB Program and its framing), and its replies are
 * capped below that.  A request that fills the buffer is too big, and is
 * discarded through to its short packet.  A multiple of the packet size, as
 * an OUT buffer has to be.
 */
#define MINDER_BUF_SIZE 4224
#define MINDER_MPS 64U

/* In lib.rs's minder_usb module. */
extern void minder_usb_ready_callback(bool ready);
extern void minder_usb_read_callback(int result);
extern void minder_usb_write_callback(int result);

NET_BUF_POOL_FIXED_DEFINE(minder_pool, 2, 0, sizeof(struct udc_buf_info), NULL);
UDC_STATIC_BUF_DEFINE(minder_out_data, MINDER_BUF_SIZE);
UDC_STATIC_BUF_DEFINE(minder_in_data, MINDER_BUF_SIZE);

enum {
	/* The host has selected a configuration with this interface in it. */
	MINDER_ENABLED,
	/* An OUT buffer is queued, or holds a request Rust has not yet copied. */
	MINDER_OUT_BUSY,
	/* A reply is being sent. */
	MINDER_IN_BUSY,
	/* The request arriving is too big, and is being dropped. */
	MINDER_DISCARD,
};

static atomic_t minder_state;
static struct usbd_class_data *minder_c_data;

/* A received request, held until minder_usb_read_copy(). */
static struct net_buf *minder_out_buf;

struct minder_desc {
	struct usb_if_descriptor if0;
	struct usb_ep_descriptor if0_out_ep;
	struct usb_ep_descriptor if0_in_ep;
	struct usb_desc_header nil_desc;
};

/* The interface number and endpoint addresses are assigned by the stack. */
static struct minder_desc minder_desc = {
	.if0 = {
		.bLength = sizeof(struct usb_if_descriptor),
		.bDescriptorType = USB_DESC_INTERFACE,
		.bInterfaceNumber = 0,
		.bAlternateSetting = 0,
		.bNumEndpoints = 2,
		.bInterfaceClass = USB_BCC_VENDOR,
		.bInterfaceSubClass = 0,
		.bInterfaceProtocol = 0,
		.iInterface = 0,
	},
	.if0_out_ep = {
		.bLength = sizeof(struct usb_ep_descriptor),
		.bDescriptorType = USB_DESC_ENDPOINT,
		.bEndpointAddress = 0x01,
		.bmAttributes = USB_EP_TYPE_BULK,
		.wMaxPacketSize = sys_cpu_to_le16(MINDER_MPS),
		.bInterval = 0x00,
	},
	.if0_in_ep = {
		.bLength = sizeof(struct usb_ep_descriptor),
		.bDescriptorType = USB_DESC_ENDPOINT,
		.bEndpointAddress = 0x81,
		.bmAttributes = USB_EP_TYPE_BULK,
		.wMaxPacketSize = sys_cpu_to_le16(MINDER_MPS),
		.bInterval = 0x00,
	},
	.nil_desc = {
		.bLength = 0,
		.bDescriptorType = 0,
	},
};

/* Full speed only: the RP2040 has no high-speed controller. */
static const struct usb_desc_header *const minder_fs_desc[] = {
	(struct usb_desc_header *)&minder_desc.if0,
	(struct usb_desc_header *)&minder_desc.if0_in_ep,
	(struct usb_desc_header *)&minder_desc.if0_out_ep,
	(struct usb_desc_header *)&minder_desc.nil_desc,
};

static struct net_buf *minder_buf_alloc(uint8_t ep, uint8_t *data)
{
	struct net_buf *buf;
	struct udc_buf_info *bi;

	buf = net_buf_alloc_with_data(&minder_pool, data, MINDER_BUF_SIZE, K_NO_WAIT);
	if (buf == NULL) {
		return NULL;
	}

	net_buf_reset(buf);
	bi = udc_get_buf_info(buf);
	memset(bi, 0, sizeof(*bi));
	bi->ep = ep;

	return buf;
}

/* Queue an OUT buffer, fresh or already used, for the next request. */
static int minder_out_queue(struct net_buf *buf)
{
	struct udc_buf_info *bi = udc_get_buf_info(buf);
	int err;

	net_buf_reset(buf);
	memset(bi, 0, sizeof(*bi));
	bi->ep = minder_desc.if0_out_ep.bEndpointAddress;

	err = usbd_ep_enqueue(minder_c_data, buf);
	if (err) {
		net_buf_unref(buf);
	}

	return err;
}

/*
 * Start receiving the next request.  minder_usb_read_callback() is called with
 * its length, or a negative errno, when it is in.  -EBUSY if a read is already
 * under way or its request has not been copied, -ENODEV if the host has not
 * configured the interface.
 */
int minder_usb_read_start(void)
{
	struct net_buf *buf;
	int err;

	if (!atomic_test_bit(&minder_state, MINDER_ENABLED)) {
		return -ENODEV;
	}

	if (atomic_test_and_set_bit(&minder_state, MINDER_OUT_BUSY)) {
		return -EBUSY;
	}

	buf = minder_buf_alloc(minder_desc.if0_out_ep.bEndpointAddress, minder_out_data);
	if (buf == NULL) {
		atomic_clear_bit(&minder_state, MINDER_OUT_BUSY);
		return -ENOMEM;
	}

	err = minder_out_queue(buf);
	if (err) {
		atomic_clear_bit(&minder_state, MINDER_OUT_BUSY);
	}

	return err;
}

/*
 * Copy out the request minder_usb_read_callback() announced, up to `cap`
 * bytes, and release its buffer for the next read.  Returns the bytes copied.
 */
size_t minder_usb_read_copy(uint8_t *dst, size_t cap)
{
	struct net_buf *buf = minder_out_buf;
	size_t len;

	if (buf == NULL) {
		return 0;
	}

	len = MIN(buf->len, cap);
	memcpy(dst, buf->data, len);

	minder_out_buf = NULL;
	net_buf_unref(buf);
	atomic_clear_bit(&minder_state, MINDER_OUT_BUSY);

	return len;
}

/*
 * Send a reply.  It is copied before this returns; minder_usb_write_callback()
 * is called when it has gone.  -EBUSY if a reply is still being sent.
 */
int minder_usb_write(const uint8_t *src, size_t len)
{
	struct net_buf *buf;
	int err;

	if (!atomic_test_bit(&minder_state, MINDER_ENABLED)) {
		return -ENODEV;
	}

	if (len == 0 || len > MINDER_BUF_SIZE) {
		return -EMSGSIZE;
	}

	if (atomic_test_and_set_bit(&minder_state, MINDER_IN_BUSY)) {
		return -EBUSY;
	}

	buf = minder_buf_alloc(minder_desc.if0_in_ep.bEndpointAddress, minder_in_data);
	if (buf == NULL) {
		atomic_clear_bit(&minder_state, MINDER_IN_BUSY);
		return -ENOMEM;
	}

	net_buf_add_mem(buf, src, len);

	/* A full last packet does not end the message; a zero-length one does. */
	if (len % MINDER_MPS == 0) {
		udc_ep_buf_set_zlp(buf);
	}

	err = usbd_ep_enqueue(minder_c_data, buf);
	if (err) {
		net_buf_unref(buf);
		atomic_clear_bit(&minder_state, MINDER_IN_BUSY);
	}

	return err;
}

static int minder_request(struct usbd_class_data *const c_data,
			  struct net_buf *buf, int err)
{
	struct udc_buf_info *bi = udc_get_buf_info(buf);

	ARG_UNUSED(c_data);

	if (bi->ep == minder_desc.if0_in_ep.bEndpointAddress) {
		net_buf_unref(buf);
		atomic_clear_bit(&minder_state, MINDER_IN_BUSY);
		minder_usb_write_callback(err);
		return 0;
	}

	if (err) {
		/* Cancelled, as when the host deconfigures us, or failed. */
		net_buf_unref(buf);
		atomic_clear_bit(&minder_state, MINDER_DISCARD);
		atomic_clear_bit(&minder_state, MINDER_OUT_BUSY);
		minder_usb_read_callback(err);
		return 0;
	}

	/*
	 * A full buffer is a request that has not ended yet, and no request this
	 * firmware accepts is that long.  Drop it, and keep reading until its
	 * short packet so the next request starts in step.
	 */
	if (buf->len == MINDER_BUF_SIZE) {
		if (!atomic_test_and_set_bit(&minder_state, MINDER_DISCARD)) {
			LOG_WRN("Minder request too large, discarding");
		}
	} else if (!atomic_test_and_clear_bit(&minder_state, MINDER_DISCARD)) {
		minder_out_buf = buf;
		minder_usb_read_callback(buf->len);
		return 0;
	}

	err = minder_out_queue(buf);
	if (err) {
		atomic_clear_bit(&minder_state, MINDER_DISCARD);
		atomic_clear_bit(&minder_state, MINDER_OUT_BUSY);
		minder_usb_read_callback(err);
	}

	return 0;
}

static const void *minder_get_desc(struct usbd_class_data *const c_data,
				   const enum usbd_speed speed)
{
	ARG_UNUSED(c_data);
	ARG_UNUSED(speed);

	return minder_fs_desc;
}

static void minder_enable(struct usbd_class_data *const c_data)
{
	ARG_UNUSED(c_data);

	LOG_INF("Minder interface enabled");
	atomic_set_bit(&minder_state, MINDER_ENABLED);
	minder_usb_ready_callback(true);
}

static void minder_disable(struct usbd_class_data *const c_data)
{
	ARG_UNUSED(c_data);

	LOG_INF("Minder interface disabled");
	atomic_clear_bit(&minder_state, MINDER_ENABLED);
	minder_usb_ready_callback(false);
}

static int minder_init(struct usbd_class_data *c_data)
{
	minder_c_data = c_data;
	return 0;
}

static const struct usbd_class_api minder_api = {
	.request = minder_request,
	.get_desc = minder_get_desc,
	.enable = minder_enable,
	.disable = minder_disable,
	.init = minder_init,
};

USBD_DEFINE_CLASS(jolt_minder, &minder_api, NULL, NULL);
