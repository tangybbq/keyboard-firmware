#!/usr/bin/env bash

# Build jolt for the mesa3w: the mesa3's matrix on a Seeed XIAO nRF52840 Plus.
# The image is build/zephyr/zephyr.uf2, for the XIAO's UF2 bootloader, or
# zephyr.hex for a J-Link on the Tag-Connect.  The board-info block is flashed
# separately; see bbq-tool/sides.sh.

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

rm -rf build
west build \
	-b xiao_ble \
	--shield mesa3w \
	-- \
	-DUSER_CACHE_DIR="$cache_dir" \
	-DBOARD_FLASH_RUNNER=jlink \
	-DBOARD_DEBUG_RUNNER=jlink \
	-DEXTRA_ZEPHYR_MODULES="$script_dir/bbqboards" \
	"$script_dir"
