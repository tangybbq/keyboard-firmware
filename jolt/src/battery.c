/*
 * The battery's charge, on the boards with one.
 *
 * The devicetree's `battery` node is a composite fuel gauge over the
 * module's battery divider: it reads the cell's voltage and turns it into a
 * charge through a stock Li-po curve.  That is an estimate from voltage
 * alone.  See jolt-mesa.md, "Bluetooth on the mesa3w".
 *
 * Whether the cell is charging comes from the charger's CHG output (the
 * `charging-gpios` of `zephyr,user`), and whether USB is powering the board
 * from the nRF52840's own VBUS detector.  Charging lifts the cell's voltage,
 * so the voltage alone would claim full before it is; while charging the
 * charge is held under 100%, and once the charger has finished it is 100%.
 *
 * The cell changes slowly, so it is read once a minute, on the system
 * workqueue; the read is a single ADC sample and two pins.  Each change goes
 * to the Battery Service, which notifies a subscribed host.  The first
 * reading comes before Bluetooth is up; the service keeps it for the host to
 * read.
 */

#include <hal/nrf_power.h>
#include <zephyr/bluetooth/services/bas.h>
#include <zephyr/device.h>
#include <zephyr/drivers/fuel_gauge.h>
#include <zephyr/drivers/gpio.h>
#include <zephyr/init.h>
#include <zephyr/kernel.h>

#include <zephyr/logging/log.h>
LOG_MODULE_REGISTER(jolt_battery, LOG_LEVEL_INF);

#define BATTERY_PERIOD K_SECONDS(60)

static const struct device *const gauge = DEVICE_DT_GET(DT_NODELABEL(battery));

static const struct gpio_dt_spec charging_pin =
	GPIO_DT_SPEC_GET(DT_PATH(zephyr_user), charging_gpios);

/* Where the board's power is coming from. */
enum power {
	/* No USB: running from the cell. */
	POWER_BATTERY,
	/* USB, and the charger is charging the cell. */
	POWER_CHARGING,
	/* USB, and the charger has finished. */
	POWER_CHARGED,
};

static const char *const power_names[] = {
	[POWER_BATTERY] = "on battery",
	[POWER_CHARGING] = "charging",
	[POWER_CHARGED] = "charged",
};

static struct k_work_delayable battery_work;

/* The charge and power last reported, so that only a change is; -1 before
 * the first. */
static int last_charge = -1;
static int last_power = -1;

static enum power read_power(void)
{
	if (!nrf_power_usbregstatus_vbusdet_get(NRF_POWER)) {
		return POWER_BATTERY;
	}
	/* With no cell the charger keeps restarting, and this flickers. */
	return gpio_pin_get_dt(&charging_pin) > 0 ? POWER_CHARGING : POWER_CHARGED;
}

static void battery_handler(struct k_work *work)
{
	ARG_UNUSED(work);

	union fuel_gauge_prop_val voltage;
	union fuel_gauge_prop_val charge;
	int err = fuel_gauge_get_prop(gauge, FUEL_GAUGE_VOLTAGE_UV, &voltage);

	if (err == 0) {
		err = fuel_gauge_get_prop(gauge, FUEL_GAUGE_RELATIVE_STATE_OF_CHARGE_PCT, &charge);
	}
	if (err != 0) {
		LOG_WRN("Battery read failed: %d", err);
		goto again;
	}

	enum power power = read_power();
	int level = charge.relative_state_of_charge_pct;

	if (power == POWER_CHARGING) {
		level = MIN(level, 99);
	} else if (power == POWER_CHARGED) {
		level = 100;
	}

	if (level != last_charge || power != last_power) {
		last_charge = level;
		last_power = power;
		LOG_INF("Battery %d mV, %d%%, %s", voltage.voltage_uv / 1000, level,
			power_names[power]);
		/* Fails only to notify, before Bluetooth is up; the level is kept. */
		if (IS_ENABLED(CONFIG_BT_BAS)) {
			(void)bt_bas_set_battery_level(level);
		}
	}

again:
	k_work_reschedule(&battery_work, BATTERY_PERIOD);
}

static int battery_init(void)
{
	if (!device_is_ready(gauge)) {
		LOG_ERR("Battery gauge not ready");
		return -ENODEV;
	}
	if (!gpio_is_ready_dt(&charging_pin) ||
	    gpio_pin_configure_dt(&charging_pin, GPIO_INPUT) != 0) {
		LOG_ERR("Charge status pin not ready");
		return -ENODEV;
	}

	k_work_init_delayable(&battery_work, battery_handler);
	k_work_reschedule(&battery_work, K_NO_WAIT);
	return 0;
}

SYS_INIT(battery_init, APPLICATION, CONFIG_APPLICATION_INIT_PRIORITY);
