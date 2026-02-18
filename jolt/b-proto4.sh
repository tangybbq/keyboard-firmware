# Build for the proto4

rm -rf build
west build \
	-b tiny2040 \
	--shield proto4 \
	-- \
	-DBOARD_FLASH_RUNNER=jlink \
	-DBOARD_DEBUG_RUNNER=jlink \
	-DEXTRA_ZEPHYR_MODULES=$PWD/bbqboards \
	.
