#! /bin/bash

# Generate the sides.

uf2conv=$ZEPHYR_BASE/scripts/build/uf2conv.py

cargo run -- board-info -o proto3-left.bin --name proto3 --side left
cargo run -- board-info -o proto3-right.bin --name proto3 --side right
cargo run -- board-info -o jolt1-left.bin --name jolt1 --side left
cargo run -- board-info -o jolt1-right.bin --name jolt1 --side right
cargo run -- board-info -o proto4.bin --name proto4

arm-zephyr-eabi-objcopy \
	-I binary \
	-O elf32-littlearm \
	--change-section-address .data=0x101fff00 \
	proto3-left.bin proto3-left.elf

arm-zephyr-eabi-objcopy \
	-I binary \
	-O elf32-littlearm \
	--change-section-address .data=0x101fff00 \
	proto3-right.bin proto3-right.elf

arm-zephyr-eabi-objcopy \
	-I binary \
	-O elf32-littlearm \
	--change-section-address .data=0x101fff00 \
	jolt1-left.bin jolt1-left.elf

arm-zephyr-eabi-objcopy \
	-I binary \
	-O elf32-littlearm \
	--change-section-address .data=0x101fff00 \
	jolt1-right.bin jolt1-right.elf

arm-zephyr-eabi-objcopy \
	-I binary \
	-O elf32-littlearm \
	--change-section-address .data=0x101fff00 \
	proto4.bin proto4.elf

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

$uf2conv \
	-b 0x101fff00 \
	-f 0xe48bff56 \
	-c \
	-o proto3-left.uf2 \
	jolt1-left.bin

$uf2conv \
	-b 0x101fff00 \
	-f 0xe48bff56 \
	-c \
	-o proto3-right.uf2 \
	jolt1-right.bin

$uf2conv \
	-b 0x101fff00 \
	-f 0xe48bff56 \
	-c \
	-o proto4.uf2 \
	proto4.bin

$uf2conv \
	-b 0x101fff00 \
	-f 0xe48bff56 \
	-c \
	-o jolt1-left.uf2 \
	jolt1-left.bin

$uf2conv \
	-b 0x101fff00 \
	-f 0xe48bff56 \
	-c \
	-o jolt1-right.uf2 \
	jolt1-right.bin
