/*
 * Deep sleep: the nRF52840's System OFF, after long enough without a key
 * (lib.rs's SLEEP_AFTER), and only on battery.  See jolt-mesa.md, "Sleep".
 *
 * In System OFF only the GPIO sense logic is awake.  Pins keep their
 * configuration and output levels, so driving every row of the matrix (the
 * pins in `col-gpios`, which the scanner drives) and arming sense on every
 * column (the pins in `row-gpios`) lets any key wake the chip, as key_wait()
 * does with interrupts while running.  Waking is a reset: the keyboard boots,
 * reconnects to the profile's host from settings, and the key that woke it
 * goes nowhere, as there is no host to send it to until then.
 *
 * The LEDs need nothing: their PWM pins idle high (`nordic,invert`), which
 * is dark, and that level is what the pins hold once the PWM stops.
 */

#include <hal/nrf_power.h>
#include <zephyr/device.h>
#include <zephyr/drivers/gpio.h>
#include <zephyr/drivers/hwinfo.h>
#include <zephyr/init.h>
#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/sys/poweroff.h>

LOG_MODULE_REGISTER(jolt_sleep, LOG_LEVEL_INF);

/* In ble.c. */
extern void ble_power_off(void);

#define MATRIX DT_ALIAS(matrix)
#define MATRIX_PIN(node, prop, idx) GPIO_DT_SPEC_GET_BY_IDX(node, prop, idx),

/* What the scanner drives, and what it senses. */
static const struct gpio_dt_spec drives[] = {DT_FOREACH_PROP_ELEM(MATRIX, col_gpios, MATRIX_PIN)};
static const struct gpio_dt_spec senses[] = {DT_FOREACH_PROP_ELEM(MATRIX, row_gpios, MATRIX_PIN)};

/* Whether the keyboard may sleep: not while USB powers it. */
bool jolt_sleep_allowed(void)
{
	return !nrf_power_usbregstatus_vbusdet_get(NRF_POWER);
}

FUNC_NORETURN void jolt_sleep(void)
{
	LOG_INF("Sleeping; any key wakes the keyboard");

	if (IS_ENABLED(CONFIG_BT)) {
		ble_power_off();
	}

	for (size_t i = 0; i < ARRAY_SIZE(drives); i++) {
		(void)gpio_pin_set_dt(&drives[i], 1);
	}
	/* A level interrupt is what the nRF GPIO driver arms sense for.  A key
	 * already down wakes the chip at once, which only costs a reboot. */
	for (size_t i = 0; i < ARRAY_SIZE(senses); i++) {
		(void)gpio_pin_interrupt_configure_dt(&senses[i], GPIO_INT_LEVEL_ACTIVE);
	}

	/* The reset reasons accumulate until cleared; clear them so the next
	 * boot's say only what woke it. */
	(void)hwinfo_clear_reset_cause();
	sys_poweroff();
}

/* Say when a boot is a wake from sleep, for the RTT log. */
static int sleep_init(void)
{
	uint32_t cause;

	if (hwinfo_get_reset_cause(&cause) == 0 && (cause & RESET_LOW_POWER_WAKE)) {
		LOG_INF("Woke from sleep");
	}
	(void)hwinfo_clear_reset_cause();
	return 0;
}

SYS_INIT(sleep_init, APPLICATION, CONFIG_APPLICATION_INIT_PRIORITY);
