#! /bin/bash

set -e

# TODO: Indicate where in the order the code implemented dictionaries: taipo, symbols, etc are to be
# placed.

uf2conv=$ZEPHYR_BASE/scripts/build/uf2conv.py


cargo run -- build -o dicts.bin \
	~/plover/phoenix.rtf \
	~/plover/phoenix_fix.json \
	+emily-symbols \
	~/plover/taipo.json \
	~/plover/user.json \
	~/plover/rust.yaml

$uf2conv \
	-b 0x10200000 \
	-f 0xe48bf556 \
	-c \
	-o dicts.uf2 \
	dicts.bin

# The elf file can be loaded with gdb, though.
arm-zephyr-eabi-objcopy \
	-I binary \
	-O elf32-littlearm \
	--change-section-address .data=0x10200000 \
	dicts.bin dicts.elf
