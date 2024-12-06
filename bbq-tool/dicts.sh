#! /bin/bash

set -e

# TODO: Indicate where in the order the code implemented dictionaries: taipo, symbols, etc are to be
# placed.

uf2conv=$ZEPHYR_BASE/scripts/build/uf2conv.py

main_addr=0x10300000
user_addr=0x10200000

cargo run -- build -o dicts.bin \
	~/plover/phoenix.rtf \
	~/plover/phoenix_fix.json \
	+emily-symbols \
	~/plover/taipo.json

cargo run -- build -o user-dict.bin \
	~/plover/user.json \
	~/plover/rust.yaml

# Full is used by host tools.
cargo run -- build -o full.bin \
	~/plover/phoenix.rtf \
	~/plover/phoenix_fix.json \
	+emily-symbols \
	~/plover/taipo.json \
	~/plover/user.json \
	~/plover/rust.yaml

$uf2conv \
	-b $main_addr \
	-f 0xe48bf556 \
	-c \
	-o dicts.uf2 \
	dicts.bin

$uf2conv \
	-b $user_addr \
	-f 0xe48bf556 \
	-c \
	-o user-dict.uf2 \
	user-dict.bin

# The elf file can be loaded with gdb, though.
arm-zephyr-eabi-objcopy \
	-I binary \
	-O elf32-littlearm \
	--change-section-address .data=$main_addr \
	dicts.bin dicts.elf

arm-zephyr-eabi-objcopy \
	-I binary \
	-O elf32-littlearm \
	--change-section-address .data=$user_addr \
	user-dict.bin user-dict.elf
