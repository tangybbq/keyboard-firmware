#!/usr/bin/env bash

# Build jolt for the mesa3w: the mesa3's matrix on a Seeed XIAO nRF52840 Plus.
# A sysbuild of MCUboot and jolt; see jolt-mesa.md, "Updates over the air".
#
#   build/mcuboot/zephyr/zephyr.elf      MCUboot, for the J-Link
#   build/jolt/zephyr/zephyr.slot0.hex   jolt, for the J-Link
#   build/jolt/zephyr/zephyr.signed.bin  jolt, for an update
#
# gdb.sh's `zb` and `z` load the first two.
#
# Images are signed with mcuboot-p256.pem in this directory, which is kept
# out of git; MCUboot only boots images signed with it.  The board-info block
# is flashed separately; see bbq-tool/sides.sh.

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$script_dir"
cache_dir="$script_dir/.cache/zephyr"
mkdir -p "$cache_dir"
export CCACHE_DISABLE=1

# The agent shell does not automatically load direnv, so bootstrap the Zephyr
# environment from the checked-in project config when needed.
if ! command -v west >/dev/null 2>&1 || [[ -z "${ZEPHYR_BASE:-}" ]] ||
	[[ "$(uname -s)" == Darwin && -z "${BINDGEN_EXTRA_CLANG_ARGS:-}" ]]; then
	source "$script_dir/../.envrc"
fi

# Zephyr's manifest pins zephyr-lang-rust at a revision from before the device
# bindings jolt uses, so a `west update` puts the module back where jolt does
# not build, with cargo errors that do not say why.  Restore it with
# `jj new main@upstream` in the module.  This goes away once Zephyr's
# submanifests/optional.yaml moves the pin.
lang_rust="$(west list -f '{abspath}' zephyr-lang-rust)"
if [[ ! -f "$lang_rust/zephyr/src/device/led_strip.rs" ]]; then
	echo "error: $lang_rust is too old for jolt (no device::led_strip)." >&2
	echo "       Run 'jj new main@upstream' there; west update resets it." >&2
	exit 1
fi

key="$script_dir/mcuboot-p256.pem"
if [[ ! -f "$key" ]]; then
	echo "error: no signing key at $key." >&2
	echo "       Make one with 'imgtool keygen -t ecdsa-p256 -k $key'," >&2
	echo "       but a new key only boots once MCUboot is reflashed with it." >&2
	exit 1
fi

shield_dir="$script_dir/bbqboards/boards/shields/mesa3w"

# The shield is jolt's alone (`jolt_SHIELD`, not `--shield`, which sysbuild
# hands to every image): its overlay and its Kconfig, Bluetooth and all, are
# not MCUboot's.  MCUboot gets just the partition table.
rm -rf build
west build \
	--sysbuild \
	-b xiao_ble \
	"$script_dir" \
	-- \
	-Djolt_SHIELD=mesa3w \
	-DSB_CONF_FILE="$shield_dir/sysbuild.conf" \
	-DSB_CONFIG_BOOT_SIGNATURE_KEY_FILE="\"$key\"" \
	-Dmcuboot_EXTRA_CONF_FILE="$shield_dir/mcuboot.conf" \
	-Dmcuboot_EXTRA_DTC_OVERLAY_FILE="$shield_dir/mesa3w-partitions.dtsi" \
	-DUSER_CACHE_DIR="$cache_dir" \
	-DBOARD_FLASH_RUNNER=jlink \
	-DBOARD_DEBUG_RUNNER=jlink \
	-DEXTRA_ZEPHYR_MODULES="$script_dir/bbqboards"

# The build signs jolt for slot 1, where an update goes: its padded images
# end at slot 1's size, a sector short of slot 0's end, where MCUboot keeps
# slot 0's trailer.  For the J-Link, sign it again padded to the whole of
# slot 0 and marked permanent.  Loading that rewrites slot 0's trailer as
# well, so one left by an earlier update, which could have MCUboot revert
# to whatever is in slot 1, never survives.  The rest of the arguments are
# the build's own (the version, the 0x200 header, the nRF52840's 4-byte
# writes).
#
# imgtool's hex output leaves the padding out, so the J-Link would rewrite
# only the sectors with code in them and the last one, and the trailer is
# two sectors long (256 sectors of swap status).  Signing to a bin and
# converting that keeps every 0xff, and the J-Link erases the whole slot.
dt_header=build/jolt/zephyr/include/generated/zephyr/devicetree_generated.h
slot0=$(awk '$2 == "DT_N_NODELABEL_slot0_partition" { print $3 }' "$dt_header")
slot0_size=$(awk -v m="${slot0}_REG_IDX_0_VAL_SIZE" '$2 == m { print $3 }' "$dt_header")
version=$(sed -n 's/^CONFIG_MCUBOOT_IMGTOOL_SIGN_VERSION="\(.*\)"$/\1/p' build/jolt/zephyr/.config)
imgtool sign \
	--version "$version" \
	--header-size 0x200 \
	--slot-size "$slot0_size" \
	--align 4 \
	--key "$key" \
	--pad --confirm \
	build/jolt/zephyr/zephyr.bin build/jolt/zephyr/zephyr.slot0.bin
slot0_addr=$(awk -v m="${slot0}_REG_IDX_0_VAL_ADDRESS" '$2 == m { print $3 }' "$dt_header")
objcopy=$(sed -n 's/^CMAKE_OBJCOPY:FILEPATH=//p' build/jolt/CMakeCache.txt)
"$objcopy" -I binary -O ihex --change-addresses "$slot0_addr" \
	build/jolt/zephyr/zephyr.slot0.bin build/jolt/zephyr/zephyr.slot0.hex
