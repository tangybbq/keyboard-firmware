#! /bin/bash

# Generate the sides.

uf2conv=$ZEPHYR_BASE/scripts/build/uf2conv.py

# Generate the files for the given side.  Arguments are:
# $1 - the name
# $2.. - Arguments passed to the board-info command.
gen_files() {
  name=$1
  shift
  echo "info: $name" "$@"

  # Generate the image.
  cargo run -- board-info -o data/$name.bin "$@"

  # And, generate a loadable elf file.  This works with 'load name.elf' in gdb.
  arm-zephyr-eabi-objcopy \
    -I binary \
    -O elf32-littlearm \
    --change-section-address .data=0x101fff00 \
    data/$name.bin data/$name.elf

  # Also generate a uf2 file.  This could theoretically be loaded with the bootloader, but it
  # doesn't seem to like data occurring too late.
  $uf2conv \
    -b 0x101fff00 \
    -f 0xe48bff56 \
    -c \
    -o data/$name.uf2 \
    data/$name.bin
}

gen_files proto3-left --name proto3 --side left
gen_files proto3-right --name proto3 --side right
gen_files proto4 --name proto4
gen_files jolt1-left --name jolt1 --side left
gen_files jolt1-right --name jolt1 --side right
gen_files jolt2dir-left --name jolt2dir --side left
gen_files jolt2-left --name jolt2 --side left
gen_files jolt2-right --name jolt2 --side right
gen_files jolt3-left --name jolt3 --side left
gen_files jolt3-right --name jolt3 --side right
