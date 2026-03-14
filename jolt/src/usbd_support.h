#ifndef JOLT_USBD_SUPPORT_H_
#define JOLT_USBD_SUPPORT_H_

#include <zephyr/usb/usbd.h>

struct usbd_context *jolt_usbd_init_device(usbd_msg_cb_t msg_cb);
struct usbd_context *jolt_usbd_setup_device(usbd_msg_cb_t msg_cb);

#endif
