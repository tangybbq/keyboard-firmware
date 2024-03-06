// GPIO to devicetree bindings.

#include <zephyr/kernel.h>
#include <zephyr/drivers/gpio.h>

// TODO: Figure out how to iterate for this with the DT macros.  For now, just
// differ.

#define MATRIX DT_ALIAS(matrix)
static const struct gpio_dt_spec row1 = GPIO_DT_SPEC_GET_BY_IDX(MATRIX, row_gpios, 0);
static const struct gpio_dt_spec row2 = GPIO_DT_SPEC_GET_BY_IDX(MATRIX, row_gpios, 1);
static const struct gpio_dt_spec row3 = GPIO_DT_SPEC_GET_BY_IDX(MATRIX, row_gpios, 2);

static const struct gpio_dt_spec col1 = GPIO_DT_SPEC_GET_BY_IDX(MATRIX, col_gpios, 0);
static const struct gpio_dt_spec col2 = GPIO_DT_SPEC_GET_BY_IDX(MATRIX, col_gpios, 1);
static const struct gpio_dt_spec col3 = GPIO_DT_SPEC_GET_BY_IDX(MATRIX, col_gpios, 2);
static const struct gpio_dt_spec col4 = GPIO_DT_SPEC_GET_BY_IDX(MATRIX, col_gpios, 3);
static const struct gpio_dt_spec col5 = GPIO_DT_SPEC_GET_BY_IDX(MATRIX, col_gpios, 4);

const struct gpio_dt_spec *matrix_rows[3] = {
    &row1, &row2, &row3,
};
const uint32_t n_matrix_rows = 3;

const struct gpio_dt_spec *matrix_cols[5] = {
	&col1, &col2, &col3, &col4, &col5,
};
const uint32_t n_matrix_cols = 5;
