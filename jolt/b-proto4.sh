#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$script_dir"
cache_dir="$script_dir/.cache/zephyr"
mkdir -p "$cache_dir"
export CCACHE_DISABLE=1

# The agent shell does not automatically load direnv, so bootstrap the Zephyr
# environment from the checked-in project config when needed.
if ! command -v west >/dev/null 2>&1 || [[ -z "${ZEPHYR_BASE:-}" ]]; then
	source "$script_dir/.envrc"
fi

rm -rf build
west build \
	-b tiny2040 \
	--shield proto4 \
	-- \
	-DUSER_CACHE_DIR="$cache_dir" \
	-DBOARD_FLASH_RUNNER=jlink \
	-DBOARD_DEBUG_RUNNER=jlink \
	-DEXTRA_ZEPHYR_MODULES="$script_dir/bbqboards" \
	"$script_dir"
