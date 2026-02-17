// HID Report.
//
// Start by using the HID Descriptor macros defined in Zephyr.  This could be eliminated entirely by
// buiding the descriptors in Rust, but that will come later.

#include <zephyr/kernel.h>
#include <zephyr/usb/class/usb_hid.h>

// Use a basic keyboard HID report for boot mode.  As long as we aren't doing
// NKRO, this should be adequate.
static const uint8_t hid_kbd_report_desc[] = HID_KEYBOARD_REPORT_DESC();

// Return this to the Rust world.
struct u8_vec {
	const uint8_t *base;
	size_t len;
};

struct u8_vec hid_get_kbd_desc(void) {
	return ((struct u8_vec){
		.base = hid_kbd_report_desc,
		.len = sizeof(hid_kbd_report_desc),
		});
}
