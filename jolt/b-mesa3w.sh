#!/usr/bin/env bash

# Build jolt for the mesa3w: the mesa3's matrix on a Seeed XIAO nRF52840 Plus.
# A sysbuild of MCUboot and jolt; see jolt-mesa.md, "Updates over the air".
#
#   build/mcuboot/zephyr/zephyr.elf      MCUboot, for the J-Link
#   build/jolt/zephyr/zephyr.slot0.hex   jolt, for the J-Link
#   build/jolt/zephyr/zephyr.signed.bin  jolt, for an update
#
# gdb.sh's `zb` and `z` load the first two.  This script configures from
# scratch; after a change, `west build` (or `ninja -C build`) in this
# directory rebuilds what changed, zephyr.slot0.hex included (slot0.cmake).
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

