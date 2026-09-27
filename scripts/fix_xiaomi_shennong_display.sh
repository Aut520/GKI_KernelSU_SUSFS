#!/usr/bin/env bash
set -e

KERNEL_ROOT="${1:-$PWD}"
TARGET_CHOICE="${2:-无}"
KERNEL_VERSION="${3:-}"
ACTUAL_SUBLEVEL="${4:-}"
OS_PATCH_LEVEL="${5:-}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=========================================================="
echo "执行小米14Pro驱动补全与高内核版本屏幕闪烁修复流程"
echo "内核源码根目录 : $KERNEL_ROOT"
echo "用户修补选项   : $TARGET_CHOICE"
echo "内核版本参数   : ${KERNEL_VERSION}.${ACTUAL_SUBLEVEL} (${OS_PATCH_LEVEL})"
echo "=========================================================="

python3 "$SCRIPT_DIR/fix_xiaomi_shennong_display.py" \
    "$KERNEL_ROOT" \
    "$TARGET_CHOICE" \
    "$KERNEL_VERSION" \
    "$ACTUAL_SUBLEVEL" \
    "$OS_PATCH_LEVEL"

