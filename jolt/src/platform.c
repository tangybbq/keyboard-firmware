/*
 * Platform services for the Rust side that have no Rust binding yet.
 */

#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <zephyr/device.h>
#include <zephyr/drivers/flash.h>
#include <zephyr/drivers/hwinfo.h>
#include <zephyr/sys/reboot.h>

#if defined(CONFIG_SOC_SERIES_RP2040)
#include <hardware/structs/rosc.h>
#else
#include <zephyr/random/random.h>
#endif

/*
 * The device's unique ID.  On the RP2040 this is the flash's 64-bit unique ID,
 * the same 8 bytes jolt-embassy-rp reads with `blocking_unique_id`.  Returns
 * the number of bytes written, or a negative errno.
 */
int jolt_device_id(uint8_t *id, size_t len)
{
	return hwinfo_get_device_id(id, len);
}

/*
 * A number that differs from one boot to the next, for minder's boot_id.  It
 * only has to differ from the last run, not be unpredictable.
 *
 * On the RP2040, the ROSC's random bit, as embassy-rp's RoscRng reads it: the
 * RP2040 has no Zephyr entropy driver, and Zephyr's clock setup leaves the
 * ROSC running.  Elsewhere, Zephyr's random source, which on the nRF52840 is
 * seeded from its hardware RNG (CONFIG_ENTROPY_GENERATOR).
 */
uint64_t jolt_boot_id(void)
{
	uint64_t id = 0;

#if defined(CONFIG_SOC_SERIES_RP2040)
	for (int i = 0; i < 64; i++) {
		id = (id << 1) | (rosc_hw->randombit & 1U);
	}
#else
	sys_rand_get(&id, sizeof(id));
#endif

	return id;
}

/*
 * Erase the `erase_size` bytes of flash at `offset` (from the start of the
 * flash device), and write `len` bytes of `data` there.  On the RP2040 both
 * run with interrupts locked, as the code they would interrupt runs from
 * flash.
 */
int jolt_flash_program(uint32_t offset, const uint8_t *data, size_t len, size_t erase_size)
{
	const struct device *flash = DEVICE_DT_GET(DT_CHOSEN(zephyr_flash_controller));
	int err;

	if (!device_is_ready(flash)) {
		return -ENODEV;
	}

	err = flash_erase(flash, offset, erase_size);
	if (err || len == 0) {
		return err;
	}

	return flash_write(flash, offset, data, len);
}

/* Reset the whole chip. */
void jolt_reboot(void)
{
	sys_reboot(SYS_REBOOT_COLD);
}
