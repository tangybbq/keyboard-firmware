/*
 * The XIAO nRF52840's QSPI flash (a P25Q16H) is unused: minder writes the
 * nRF52840's own flash.  Left alone it sits in standby, which is several uA
 * on the battery around the clock, in System OFF as well, since its supply
 * stays on.  Its deep power-down is a fraction of that.  The driver only
 * enters it as a PM suspend, so suspend it once at boot and never resume it.
 * The driver's init takes the chip out of deep power-down, so a reset, or a
 * wake from System OFF, comes back through here.
 */

#include <zephyr/device.h>
#include <zephyr/init.h>
#include <zephyr/logging/log.h>
#include <zephyr/pm/device.h>

LOG_MODULE_REGISTER(jolt_qspi, LOG_LEVEL_INF);

#define QSPI_FLASH DT_INST(0, nordic_qspi_nor)

static int qspi_off_init(void)
{
	const struct device *flash = DEVICE_DT_GET(QSPI_FLASH);
	int err;

	if (!device_is_ready(flash)) {
		LOG_WRN("QSPI flash not ready; left in standby");
		return 0;
	}

	err = pm_device_action_run(flash, PM_DEVICE_ACTION_SUSPEND);
	if (err) {
		LOG_WRN("QSPI flash power-down failed: %d", err);
	}
	return 0;
}

SYS_INIT(qspi_off_init, APPLICATION, CONFIG_APPLICATION_INIT_PRIORITY);
