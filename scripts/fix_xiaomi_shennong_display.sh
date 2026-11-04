#!/usr/bin/env bash
set -e

KERNEL_ROOT="${1:-$PWD}"
IS_ENABLED="${2:-false}"
KERNEL_VERSION="${3:-}"
ACTUAL_SUBLEVEL="${4:-}"
OS_PATCH_LEVEL="${5:-}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=========================================================="
echo "执行小米14Pro驱动补全与屏幕闪烁修复流程 (版本规则: 6.1 > 140 直接放行)"
echo "内核源码根目录 : $KERNEL_ROOT"
echo "是否启用开关   : $IS_ENABLED"
echo "当前内核参数   : ${KERNEL_VERSION}.${ACTUAL_SUBLEVEL} (${OS_PATCH_LEVEL})"
echo "=========================================================="

python3 "$SCRIPT_DIR/fix_xiaomi_shennong_display.py" \
    "$KERNEL_ROOT" \
    "$IS_ENABLED" \
    "$KERNEL_VERSION" \
    "$ACTUAL_SUBLEVEL" \
    "$OS_PATCH_LEVEL"
