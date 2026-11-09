/*
 * The running image, as MCUboot sees it.  See jolt-mesa.md, "Updates over
 * the air".
 *
 * MCUboot runs swap using move: an update is swapped into slot 0 on trial,
 * and swapped back out at the next reset unless the image confirms itself
 * first.  jolt confirms once a host can reach it (image.rs), since an image
 * that no host can reach is one no update can replace.  A wake from System
 * OFF is a reset, so an image that never gets that far is reverted when it
 * next wakes, at worst 15 minutes on.
 *
 * At boot this says which image is running and whether it is confirmed, for
 * the RTT log.
 */

#include <zephyr/dfu/mcuboot.h>
#include <zephyr/init.h>
#include <zephyr/logging/log.h>
#include <zephyr/storage/flash_map.h>

LOG_MODULE_REGISTER(jolt_image, LOG_LEVEL_INF);

/* Confirm the running image, if it is on trial.  Called from the executor;
 * the write is a few bytes of slot 0's trailer, and only happens once, on
 * the first boot of an update. */
void jolt_image_confirm(void)
{
	int err;

	if (boot_is_img_confirmed()) {
		return;
	}

	err = boot_write_img_confirmed();
	if (err) {
		LOG_ERR("Image confirm failed: %d; it reverts at the next reset", err);
		return;
	}
	LOG_INF("Image confirmed");
}

static int image_init(void)
{
	struct mcuboot_img_header header;
	int err;

	err = boot_read_bank_header(PARTITION_ID(slot0_partition), &header, sizeof(header));
	if (err) {
		LOG_WRN("No image header in slot 0: %d", err);
		return 0;
	}

	const struct mcuboot_img_sem_ver *ver = &header.h.v1.sem_ver;
	LOG_INF("Image %u.%u.%u+%u, %s", ver->major, ver->minor, ver->revision, ver->build_num,
		boot_is_img_confirmed() ? "confirmed" : "on trial until a host connects");
	return 0;
}

SYS_INIT(image_init, APPLICATION, CONFIG_APPLICATION_INIT_PRIORITY);
