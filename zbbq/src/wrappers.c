// syscall wrappers.

#include <zephyr/kernel.h>
#include <zephyr/drivers/gpio.h>
#include <zephyr/spinlock.h>

void sys_k_busy_wait(uint32_t usec_to_wait)
{
	k_busy_wait(usec_to_wait);
}

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

void sys_k_timer_start(struct k_timer *timer,
                       k_timeout_t duration,
                       k_timeout_t period)
{
	k_timer_start(timer, duration, period);
}

void sys_k_timer_stop(struct k_timer *timer)
{
	k_timer_stop(timer);
}

uint32_t sys_k_timer_status_sync(struct k_timer *timer)
{
	return k_timer_status_sync(timer);
}

// Spinlock for critical sections.
static struct k_spinlock crit_lock;

uint32_t z_crit_acquire(void)
{
	return k_spin_lock(&crit_lock).key;
}

void z_crit_release(uint32_t token)
{
	k_spinlock_key_t key;
	key.key = token;
	k_spin_unlock(&crit_lock, key);
}

int sys_mutex_lock(struct k_mutex *mutex, k_timeout_t timeout)
{
	return k_mutex_lock(mutex, timeout);
}

int sys_mutex_unlock(struct k_mutex *mutex)
{
	return k_mutex_unlock(mutex);
}

int sys_condvar_signal(struct k_condvar *condvar)
{
	return k_condvar_signal(condvar);
}

int sys_condvar_broadcast(struct k_condvar *condvar)
{
	return k_condvar_broadcast(condvar);
}

int sys_condvar_wait(struct k_condvar *condvar, struct k_mutex *mutex, k_timeout_t timeout)
{
	return k_condvar_wait(condvar, mutex, timeout);
}

uint64_t sys_cycle_get_64(void)
{
	return k_cycle_get_64();
}
