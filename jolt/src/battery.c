/*
 * The battery's charge, on the boards with one.
 *
 * The devicetree's `battery` node is a composite fuel gauge over the
 * module's battery divider: it reads the cell's voltage and turns it into a
 * charge through a stock Li-po curve.  That is an estimate from voltage
 * alone, and with no charger status it reads near full whenever USB is
 * charging the cell.  See jolt-mesa.md, "Bluetooth on the mesa3w".
 *
 * The cell changes slowly, so it is read once a minute, on the system
 * workqueue; the read is a single ADC sample.
 */

#include <zephyr/device.h>
#include <zephyr/drivers/fuel_gauge.h>
#include <zephyr/init.h>
#include <zephyr/kernel.h>

#include <zephyr/logging/log.h>
LOG_MODULE_REGISTER(jolt_battery, LOG_LEVEL_INF);

#define BATTERY_PERIOD K_SECONDS(60)

static const struct device *const gauge = DEVICE_DT_GET(DT_NODELABEL(battery));

static struct k_work_delayable battery_work;

/* The charge last logged, so that only a change is; -1 before the first. */
static int last_charge = -1;

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
	} else if (charge.relative_state_of_charge_pct != last_charge) {
		last_charge = charge.relative_state_of_charge_pct;
		LOG_INF("Battery %d mV, %d%%", voltage.voltage_uv / 1000, last_charge);
	}

	k_work_reschedule(&battery_work, BATTERY_PERIOD);
}

static int battery_init(void)
{
	if (!device_is_ready(gauge)) {
		LOG_ERR("Battery gauge not ready");
		return -ENODEV;
	}

	k_work_init_delayable(&battery_work, battery_handler);
	k_work_reschedule(&battery_work, K_NO_WAIT);
	return 0;
}

SYS_INIT(battery_init, APPLICATION, CONFIG_APPLICATION_INIT_PRIORITY);
