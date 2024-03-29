/* My example */

#include "zephyr/sys/mpsc_pbuf.h"
#include "zephyr/syscall.h"
#include <zephyr/kernel.h>
#include <zephyr/logging/log_ctrl.h>
#include <stdlib.h>

#include <zephyr/app_memory/mem_domain.h>

#define LOG_LEVEL 4
#include <zephyr/logging/log.h>
LOG_MODULE_REGISTER(main);

#include <zephyr/drivers/led_strip.h>
#include <zephyr/drivers/gpio.h>

#define STRIP_NODE   DT_ALIAS(led_strip)
#define STRIP_NUM_PIXELS  DT_PROP(DT_ALIAS(led_strip), chain_length)

static const struct device *const strip = DEVICE_DT_GET(STRIP_NODE);

// Get the GPIOs for the keyboard matrix.  This is a little weird.
#define MATRIX DT_ALIAS(matrix)
static const struct gpio_dt_spec row1 = GPIO_DT_SPEC_GET_BY_IDX(MATRIX, row_gpios, 0);
static const struct gpio_dt_spec row2 = GPIO_DT_SPEC_GET_BY_IDX(MATRIX, row_gpios, 1);
static const struct gpio_dt_spec row3 = GPIO_DT_SPEC_GET_BY_IDX(MATRIX, row_gpios, 2);

static const struct gpio_dt_spec col1 = GPIO_DT_SPEC_GET_BY_IDX(MATRIX, col_gpios, 0);
static const struct gpio_dt_spec col2 = GPIO_DT_SPEC_GET_BY_IDX(MATRIX, col_gpios, 1);
static const struct gpio_dt_spec col3 = GPIO_DT_SPEC_GET_BY_IDX(MATRIX, col_gpios, 2);
static const struct gpio_dt_spec col4 = GPIO_DT_SPEC_GET_BY_IDX(MATRIX, col_gpios, 3);
static const struct gpio_dt_spec col5 = GPIO_DT_SPEC_GET_BY_IDX(MATRIX, col_gpios, 4);

static const struct gpio_dt_spec rows[3] = {
	row1, row2, row3,
};

static const struct gpio_dt_spec cols[5] = {
	col1, col2, col3, col4, col5,
};

struct matrix_info {
	const struct gpio_dt_spec *const rows;
	uint32_t nrows;
	const struct gpio_dt_spec *const cols;
	uint32_t ncols;
};

struct matrix_info get_matrix_info(void)
{
	struct matrix_info result = {
		.rows = rows,
		.cols = cols,
		.nrows = 3,
		.ncols = 5,
	};
	return result;
}

#if 0
static void wait_on_log_flushed(void)
{
	// Disable this.
	if (1) {
		return;
	}
	while (log_buffered_cnt()) {
		k_sleep(K_MSEC(5));
	}
}
#endif

const struct device *const get_led_strip(void) {
	return strip;
}

extern void rust_main(void *, void *, void *);

void msg_string(const char* msg)
{
	LOG_INF("%s", msg);
}

// Syscall wrappers.  These are just generated as C wrappers and later we will
// make this more efficient.
bool sys_device_is_ready(const struct device *dev)
{
	return device_is_ready(dev);
}

int sys_gpio_pin_configure(const struct device *port, gpio_pin_t pin,
                           gpio_flags_t flags)
{
	return gpio_pin_configure(port, pin, flags);
}

int sys_gpio_pin_set(const struct device *port, gpio_pin_t pin, int value) {
	return gpio_pin_set(port, pin, value);
}

int sys_gpio_pin_get(const struct device *port, gpio_pin_t pin) {
	return gpio_pin_get(port, pin);
}

void sys_k_timer_start(struct k_timer *timer, k_timeout_t duration, k_timeout_t period) {
	k_timer_start(timer, duration, period);
}

void sys_k_timer_stop(struct k_timer *timer) {
	k_timer_stop(timer);
}

void sys_k_timer_status_sync(struct k_timer *timer) {
	k_timer_status_sync(timer);
}

/// Wrapper for k_panic(), simple way to get past all of the macros.
void c_k_panic()
{
	k_panic();
}

/// Wrapper to sleep for n ms.
void c_k_sleep_ms(uint32_t ms)
{
	k_sleep(K_MSEC(ms));
}

void trampoline(void *a, void *b, void *c) {
	printk("trampoline\n");
	char *foo = malloc(32);
	printk("foo: %p\n", foo);
	free(foo);
	rust_main(a, b, c);
}

K_TIMER_DEFINE(ms_timer, NULL, NULL);

int main(void)
{
	char *foo = malloc(32);
	printk("foo: %p\n", foo);
	free(foo);
	printk("Sizeof k_timer: %d\n", sizeof(struct k_timer));

	// Verify my understanding of time.
	printk("Ticks: %d\n", (int)K_MSEC(1).ticks);

	// Fix domain.
#if 0
	struct z_app_region {
		void *bss_start;
		size_t bss_size;
	};
	extern struct z_app_region z_malloc_partition_region;
	struct k_mem_partition ptn;
	ptn.start = (uintptr_t)z_malloc_partition_region.bss_start;
	ptn.size = 8192;
	ptn.attr = K_MEM_PARTITION_P_RW_U_RW;
	int ret = k_mem_domain_add_partition(&k_mem_domain_default, &ptn);
	printk("add partition: %d\n", ret);
#endif

	// Jump to usermode for rust.
	// k_thread_user_mode_enter(rust_main, 0, 0, 0);
	k_thread_user_mode_enter(trampoline, 0, 0, 0);
	// rust_main();
#if 0
	register uint32_t sp __asm__("sp");
	extern uint32_t z_main_stack;

	// The stack usage already shows the usage before we got to this point.
	// To make this work a little better, clear out the stack, again.
	uint32_t base = (uint32_t)&z_main_stack;
	uint32_t len = sp - base;

	// Adjust the length to allow for the call to memset (and the base adjust).
	len -= 20;
	// Adjust the base to not overwrite the canary.
	base += 4;
	memset((void *)base, 0xaa, len);

        // LOG_INF("sp: %08x", sp);
        // LOG_INF("main: %08x", (uint32_t)&z_main_stack);

        // LOG_INF("Hello world, from C main");
        wait_on_log_flushed();

        rust_main();
        size_t left = 0;
        k_thread_stack_space_get(k_current_get(), &left);
        LOG_INF("after rust: %d bytes of stack used",
                CONFIG_MAIN_STACK_SIZE - left);
#endif

        // Log a periodic message.
        /*
        int count = 0;
        while (1) {
                count++;
                LOG_INF("Tick: %d", count);
                wait_on_log_flushed();
                k_sleep(K_MSEC(1000));
        }
        */
}
