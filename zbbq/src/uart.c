// Uart device suppport.

#include <zephyr/devicetree.h>
#include <zephyr/kernel.h>
#include <zephyr/drivers/uart.h>
#include <zephyr/sys/ring_buffer.h>

// The presence of the inter-board-uart is determined by whether this chosen
// node is present.

// Note that UART writing is blocking.  As such, we use the IRQ interface.  But,
// we don't actually need the interrupts to be enabled, since our messages fit
// comfortably within the FIFO on the UART.

#if DT_HAS_CHOSEN(inter_board_uart)

#define INTER_UART DT_CHOSEN(inter_board_uart)
static const struct device *uart = DEVICE_DT_GET
	(INTER_UART);

#if 0
// This is some legacy attempts to use the irq mode.  This isn't actually
// necessary for us, though.
RING_BUF_DECLARE(inter_ring, 64);

uint8_t ubugs[1024];
int ubug_len = 0;

// Irq handler. For received data, just sticks it in the ring.  For transmit,
// does nothing.
static void inter_uart_isr(const struct device *dev, void *user_data)
{
	ARG_UNUSED(user_data);

	uart_irq_update(dev);

	if (!uart_irq_rx_ready(dev)) {
		return;
	}

	for (;;) {
		uint8_t *data;
		uint32_t size;

		size = ring_buf_put_claim(&inter_ring, &data, 32);
		if (size == 0) {
			// Ring is full, so ignore stuff.
			// k_panic();
			break;
		}

		int got = uart_fifo_read(dev, data, size);
		if (got <= 0) {
			ring_buf_put_finish(&inter_ring, 0);
			break;
		}

		uint8_t *tmppos = data;
		uint32_t tmpsize = got;
		while (ubug_len < 1024 && tmpsize > 0) {
			ubugs[ubug_len++] = *tmppos++;
			tmpsize--;
		}

		ring_buf_put_finish(&inter_ring, got);
	}
}
#endif

int inter_uart_poll_in(unsigned char *p_char)
{
	uint32_t got;

	got = uart_fifo_read(uart, p_char, 1);
	if (got == 1) {
		return 0;
	} else {
		return -1;
	}
	/*
	// TODO: We should probably be more efficient here.
	uint32_t got;

	got = ring_buf_get(&inter_ring, p_char, 1);

	if (got == 1) {
		return 0;
	} else {
		return -1;
	}
	// return uart_poll_in(uart, p_char);
	*/
}

void inter_uart_poll_out(unsigned char out_char)
{
	// Try sending the character, discard if not sent.  It is important to
	// not block.
	uart_fifo_fill(uart, &out_char, 1);
}

void inter_uart_setup(void)
{
	uart_irq_rx_disable(uart);
	uart_irq_tx_disable(uart);

	// uart_irq_callback_set(uart, inter_uart_isr);

	// Drain the fifo.
	uint8_t ch;
	while (uart_fifo_read(uart, &ch, 1) == 1) {
	}

	// uart_irq_rx_enable(uart);
}

#else // DT_NODE_EXISTS(INTER_UART)

int inter_uart_poll_in(unsigned char *p_char)
{
	(void) p_char;
	return -1;
}

void inter_uart_poll_out(unsigned char out_char)
{
	(void) out_char;
}

void inter_uart_setup(void)
{
}

#endif
