#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$script_dir"

# Cargo uses the generated .cargo/config.toml for most Zephyr build state, but
# zephyr-sys still expects ZEPHYR_BASE to be present in the environment.
if [[ -z "${ZEPHYR_BASE:-}" ]]; then
	source "$script_dir/.envrc"
fi

exec cargo check "$@"
