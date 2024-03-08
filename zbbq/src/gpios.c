// GPIO to devicetree bindings.

#include "zephyr/devicetree.h"
#include <zephyr/kernel.h>
#include <zephyr/drivers/gpio.h>

/// GPIO expansions for the keyboard matrix.
/// This exports:
/// - matrix_rows: An array of `gpio_dt_spec *` for each row pin.
/// - n_matrix_rows: How many row pins.
/// - matrix_cols: An array of `gpio_dt_spec *` for each col pin.
/// - n_matrix_cols: How many col pins.

#define MATRIX DT_ALIAS(matrix)

#define ROW(node, prop, idx) \
	static const struct gpio_dt_spec row ## idx = GPIO_DT_SPEC_GET_BY_IDX(node, prop, idx);
DT_FOREACH_PROP_ELEM_SEP(MATRIX, row_gpios, ROW, (;));
#undef ROW

#define COL(node, prop, idx) \
	static const struct gpio_dt_spec col ## idx = GPIO_DT_SPEC_GET_BY_IDX(node, prop, idx);
DT_FOREACH_PROP_ELEM_SEP(MATRIX, col_gpios, COL, (;));
#undef COL

#define ROW(node, prop, idx) &row ## idx,
const struct gpio_dt_spec *matrix_rows[3] = {
	DT_FOREACH_PROP_ELEM(MATRIX, row_gpios, ROW)
};
const uint32_t n_matrix_rows = DT_PROP_LEN(MATRIX, row_gpios);
#undef ROW

#define COL(node, prop, idx) &col ## idx,
const struct gpio_dt_spec *matrix_cols[5] = {
	DT_FOREACH_PROP_ELEM(MATRIX, col_gpios, COL)
};
#undef COL
const uint32_t n_matrix_cols = DT_PROP_LEN(MATRIX, col_gpios);

/// GPIO for the side select detect.
#define SIDE_SELECT DT_PATH(side_select)
const struct gpio_dt_spec side_select = GPIO_DT_SPEC_GET(SIDE_SELECT, in_gpios);
