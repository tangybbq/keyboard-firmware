// Binding for LEDs.

#include <zephyr/kernel.h>
#include <zephyr/drivers/led_strip.h>

#define STRIP_NODE DT_ALIAS(led_strip)
const uint32_t strip_length = DT_PROP(STRIP_NODE, chain_length);
const struct device *const strip = DEVICE_DT_GET(STRIP_NODE);

int sys_led_strip_update_rgb(const struct device *dev,
			     struct led_rgb *pixels,
			     size_t num_pixels)
{
	return led_strip_update_rgb(dev, pixels, num_pixels);
}
