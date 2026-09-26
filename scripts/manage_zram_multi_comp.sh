#!/usr/bin/env bash
set -e

KERNEL_ROOT="${1:-$PWD}"
KERNEL_VERSION="${2:-}"
ENABLE_MULTI_COMP="${3:-false}"
PATCH_MULTI_COMP="${4:-false}"
CONFIG_FILE="${5:-}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

python3 "$SCRIPT_DIR/manage_zram_multi_comp.py" \
    "$KERNEL_ROOT" \
    "$KERNEL_VERSION" \
    "$ENABLE_MULTI_COMP" \
    "$PATCH_MULTI_COMP" \
    "$CONFIG_FILE"

