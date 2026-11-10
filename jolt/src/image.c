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
 *
 * Minder writes an update into slot 1 through here (minder.rs, image.rs).
 * Slot 1 is on the QSPI flash, which minder's Hash and Program address at
 * the nRF52840's mapping of it, 0x12000000 up (the `qspi_mm` region), so
 * that one address space covers both flashes.  Nothing reads it through
 * that mapping; the reads and writes go through the flash driver, which
 * wakes the chip from deep power-down for each.
 */

#include <errno.h>
#include <zephyr/devicetree.h>
#include <zephyr/dfu/mcuboot.h>
#include <zephyr/drivers/clock_control/nrf_clock_control.h>
#include <zephyr/init.h>
#include <zephyr/logging/log.h>
#include <zephyr/storage/flash_map.h>

LOG_MODULE_REGISTER(jolt_image, LOG_LEVEL_INF);

/* Slot 1's partition is on the QSPI flash chip, whose parent is the QSPI
 * controller, whose `qspi_mm` region is where the chip is mapped. */
#define SLOT1_NODE DT_NODELABEL(slot1_partition)
#define SLOT1_QSPI DT_PARENT(DT_GPARENT(SLOT1_NODE))
#define SLOT1_MAPPED (DT_REG_ADDR_BY_NAME(SLOT1_QSPI, qspi_mm) + DT_REG_ADDR(SLOT1_NODE))

BUILD_ASSERT(DT_NODE_HAS_COMPAT(SLOT1_QSPI, nordic_nrf_qspi),
	     "minder's window onto slot 1 assumes it is on the nRF QSPI flash");

/* What minder's ImageInfo reports; image.rs has the same layout. */
struct jolt_image_info {
	uint32_t slot_addr;
	uint32_t slot_size;
	uint8_t major;
	uint8_t minor;
	uint16_t revision;
	uint32_t build;
	bool confirmed;
};

static const struct flash_area *slot1(void)
{
	static const struct flash_area *fa;

	if (fa == NULL && flash_area_open(PARTITION_ID(slot1_partition), &fa) != 0) {
		fa = NULL;
	}
	return fa;
}

/* Where slot 1 is, as minder addresses it, and its size. */
void jolt_slot_region(uint32_t *addr, uint32_t *size)
{
	*addr = SLOT1_MAPPED;
	*size = DT_REG_SIZE(SLOT1_NODE);
}

/*
 * The nRF52840's QSPI corrupts what it moves to and from the flash when the
 * high frequency clock changes source during the transfer (anomaly 244),
 * and the Bluetooth controller starts the crystal for each radio event and
 * stops it after.  Zephyr's QSPI driver does nothing about it on the
 * nRF52840.  Here a 4 KiB write over Bluetooth lost its sixth 256-byte
 * page nearly every time, presumably the one programmed as the next
 * connection event came round.  So the crystal is held on around every use
 * of slot 1, and the clock cannot change under it.
 */
#define HFCLK DEVICE_DT_GET_ONE(nordic_nrf_clock_hfclk)

/* Start the crystal and wait until it runs, which takes about a
 * millisecond.  A success is matched by a hfxo_release(). */
static int hfxo_hold(void)
{
	int err = nrf_clock_control_request_sync(HFCLK, NULL, K_MSEC(100));

	return err < 0 ? err : 0;
}

static void hfxo_release(void)
{
	(void)nrf_clock_control_release(HFCLK, NULL);
}

/* Read from slot 1, at an offset into it. */
int jolt_slot_read(uint32_t offset, uint8_t *buf, size_t len)
{
	const struct flash_area *fa = slot1();
	int err;

	if (fa == NULL) {
		return -ENODEV;
	}
	err = hfxo_hold();
	if (err < 0) {
		return err;
	}
	err = flash_area_read(fa, offset, buf, len);
	hfxo_release();
	return err;
}

/* Erase a block of slot 1 and write `len` bytes at its start; the rest stays
 * erased.  The QSPI flash takes writes of a multiple of 4 bytes. */
int jolt_slot_program(uint32_t offset, const uint8_t *data, size_t len, size_t erase_size)
{
	const struct flash_area *fa = slot1();
	int err;

	if (fa == NULL) {
		return -ENODEV;
	}
	err = hfxo_hold();
	if (err < 0) {
		return err;
	}
	err = flash_area_erase(fa, offset, erase_size);
	if (err == 0 && len != 0) {
		err = flash_area_write(fa, offset, data, len);
	}
	hfxo_release();
	return err;
}

/* The running image's version and state, and where an update goes. */
int jolt_image_info(struct jolt_image_info *info)
{
	struct mcuboot_img_header header;
	int err;

	jolt_slot_region(&info->slot_addr, &info->slot_size);
	err = boot_read_bank_header(PARTITION_ID(slot0_partition), &header, sizeof(header));
	if (err) {
		return err;
	}
	info->major = header.h.v1.sem_ver.major;
	info->minor = header.h.v1.sem_ver.minor;
	info->revision = header.h.v1.sem_ver.revision;
	info->build = header.h.v1.sem_ver.build_num;
	info->confirmed = boot_is_img_confirmed();
	return 0;
}

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
