/* Interfacing with Zephyr's PWM driver.
 */

#include <zephyr/device.h>
#include <zephyr/devicetree.h>
#include <zephyr/kernel.h>
#include <zephyr/drivers/led.h>

// TODO: Make this conditional.
// TODO: Interface this through DT in Rust.

#define LED_PWM_NODE_ID DT_COMPAT_GET_ANY_STATUS_OKAY(pwm_leds)

const char *led_label[] = {
	DT_FOREACH_CHILD_SEP_VARGS(LED_PWM_NODE_ID, DT_PROP_OR, (,), label, NULL)
};

struct pwm_led_info {
	const struct device *dev;
	uint32_t count;
};

struct pwm_led_info get_pwm(void) {
	struct pwm_led_info info;

	info.dev = DEVICE_DT_GET(LED_PWM_NODE_ID);
	if (!device_is_ready(info.dev)) {
		info.dev = 0;
		info.count = 0;
		return info;
	}

	info.count = ARRAY_SIZE(led_label);

	return info;
}

// Manual wrapper.
int pwm_set_brightness(const struct device *dev, uint32_t index, uint8_t value) {
	return led_set_brightness(dev, index, value);
}
