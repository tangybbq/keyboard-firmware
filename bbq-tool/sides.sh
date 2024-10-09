#! /bin/bash

# Generate the sides.

uf2conv=$ZEPHYR_BASE/scripts/build/uf2conv.py

cargo run -- board-info -o proto3-left.bin --name proto3 --side left
cargo run -- board-info -o proto3-right.bin --name proto3 --side right

# arm-zephyr-eabi-objcopy \
# 	-I binary \
# 	-O elf32-littlearm \
# 	--change-section-address .data=0x1001fff00 \
# 	proto3-left.bin proto3-left.elf

# arm-zephyr-eabi-objcopy \
# 	-I binary \
# 	-O elf32-littlearm \
# 	--change-section-address .data=0x1001fff00 \
# 	proto3-right.bin proto3-right.elf

$uf2conv \
	-b 0x101fff00 \
	-f 0xe48bff56 \
	-c \
	-o proto3-left.uf2 \
	proto3-left.bin

$uf2conv \
	-b 0x101fff00 \
	-f 0xe48bff56 \
	-c \
	-o proto3-right.uf2 \
	proto3-right.bin
