#! /bin/bash

# Generate the sides.

uf2conv=$ZEPHYR_BASE/scripts/build/uf2conv.py

# Where the board info goes, and the UF2 family the bootloader takes.
#
# RP2040 boards keep it at 0x101fff00, as jolt-embassy-rp's memory.x and jolt's
# rp2040-board-info.dtsi have it.  The nRF52840 boards (the mesa3w's XIAO) keep
# it in the last page of the storage partition, below the Adafruit UF2
# bootloader, as jolt's mesa3w shield has it.
RP2040_ADDR=0x101fff00
RP2040_FAMILY=0xe48bff56
NRF52840_ADDR=0xf3000
NRF52840_FAMILY=0xada52840

# Generate the files for the given side.  Arguments are:
# $1 - the address
# $2 - the UF2 family
# $3 - the name
# $4.. - Arguments passed to the board-info command.
gen_files_at() {
  addr=$1
  family=$2
  name=$3
  shift 3
  echo "info: $name" "$@"

  # Generate the image.
  cargo run -- board-info -o data/$name.bin "$@"

  # And, generate a loadable elf file.  This works with 'load name.elf' in gdb.
  arm-zephyr-eabi-objcopy \
    -I binary \
    -O elf32-littlearm \
    --change-section-address .data=$addr \
    data/$name.bin data/$name.elf

  # An Intel hex file, for a J-Link (`loadfile name.hex`) or nrfjprog.
  arm-zephyr-eabi-objcopy \
    -I binary \
    -O ihex \
    --change-addresses $addr \
    data/$name.bin data/$name.hex

  # Also generate a uf2 file.  This could theoretically be loaded with the bootloader, but the
  # RP2040's doesn't seem to like data occurring too late.
  $uf2conv \
    -b $addr \
    -f $family \
    -c \
    -o data/$name.uf2 \
    data/$name.bin
}

gen_files() {
  gen_files_at $RP2040_ADDR $RP2040_FAMILY "$@"
}

gen_files_nrf52840() {
  gen_files_at $NRF52840_ADDR $NRF52840_FAMILY "$@"
}

gen_files proto3-left --name proto3 --side left
gen_files proto3-right --name proto3 --side right
gen_files proto4 --name proto4
gen_files mesa1 --name mesa1
gen_files mesa2 --name mesa2
gen_files mesa2b --name mesa2b
gen_files mesa3 --name mesa3
gen_files mesa3b --name mesa3b
gen_files_nrf52840 mesa3w --name mesa3w
gen_files jolt1-left --name jolt1 --side left
gen_files jolt1-right --name jolt1 --side right
gen_files jolt2dir-left --name jolt2dir --side left
gen_files jolt2-left --name jolt2 --side left
gen_files jolt2-right --name jolt2 --side right
gen_files jolt3-left --name jolt3 --side left
gen_files jolt3-right --name jolt3 --side right
