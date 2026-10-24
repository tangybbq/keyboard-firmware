/*
 * Platform services for the Rust side that have no Rust binding yet.
 */

#include <stddef.h>
#include <stdint.h>
#include <zephyr/drivers/hwinfo.h>

/*
 * The device's unique ID.  On the RP2040 this is the flash's 64-bit unique ID,
 * the same 8 bytes jolt-embassy-rp reads with `blocking_unique_id`.  Returns
 * the number of bytes written, or a negative errno.
 */
int jolt_device_id(uint8_t *id, size_t len)
{
	return hwinfo_get_device_id(id, len);
}
