// syscall wrappers.

#include <zephyr/drivers/gpio.h>

int sys_gpio_pin_configure(const struct device *port,
                           gpio_pin_t pin,
                           gpio_flags_t flags)
{
	return gpio_pin_configure(port, pin, flags);
}

int sys_gpio_pin_get(const struct device *port, gpio_pin_t pin)
{
	return gpio_pin_get(port, pin);
}

int sys_gpio_pin_set(const struct device *port, gpio_pin_t pin, int value)
{
	return gpio_pin_set(port, pin, value);
}
