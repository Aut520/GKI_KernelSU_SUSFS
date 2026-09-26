#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
ZRAM Multi-Comp (多压缩流 / 二次重压缩) 全版本兼容管理与修补脚本
支持 Linux 5.10, 5.15, 6.1, 6.6, 6.12+ 全系列内核版本。
负责处理双开关状态互锁、版本自适应检测、源码补丁注入与 defconfig 配置追加。
"""

import sys
import os
import subprocess

def log(msg):
    try:
        print(f"[Multi-Comp] {msg}", flush=True)
    except Exception:
        clean = msg.encode("ascii", errors="replace").decode("ascii")
        print(f"[Multi-Comp] {clean}", flush=True)

def append_config(config_file):
    if not config_file or not os.path.isfile(config_file):
        log(f"[WARN] 未找到配置文件: {config_file}，无法追加配置项")
        return

    configs_to_add = [
        "CONFIG_ZRAM_MULTI_COMP=y\n",
        "CONFIG_ZRAM_TRACK_ENTRY_ACTIME=y\n",
        "CONFIG_ZRAM_BACKEND_ZSTD=y\n",
        "CONFIG_CRYPTO_ZSTD=y\n",
        "CONFIG_ZRAM_WRITEBACK=y\n"
    ]

    try:
        with open(config_file, "r", encoding="utf-8", errors="replace") as f:
            content = f.read()

        new_entries = []
        for cfg in configs_to_add:
            key = cfg.split("=")[0].strip()
            if key not in content:
                new_entries.append(cfg)

        if new_entries:
            with open(config_file, "a", encoding="utf-8") as f:
                f.write("\n# ZRAM Multi-Comp (多压缩流) 功能配置\n")
                f.writelines(new_entries)
            log(f"[OK] 已成功向 {os.path.basename(config_file)} 注入 Multi-Comp 配置项")
        else:
            log(f"[INFO] {os.path.basename(config_file)} 中已存在相关配置项，无需重复注入")
    except Exception as e:
        log(f"[WARN] 注入配置文件时发生异常: {e}")

def check_native_support(kernel_root, kernel_version):
    """
    检查当前内核源码是否原生内置 Multi-Comp 驱动与 Kconfig
    """
    # 1. 检查 Kconfig 源码中是否已原生包含 ZRAM_MULTI_COMP
    kconfig_path = os.path.join(kernel_root, "drivers/block/zram/Kconfig")
    if os.path.isfile(kconfig_path):
        try:
            with open(kconfig_path, "r", encoding="utf-8", errors="replace") as f:
                if "config ZRAM_MULTI_COMP" in f.read():
                    return True
        except Exception:
            pass

    # 2. 版本号判定：主版本号 >= 6 且次版本号 >= 6 (例如 6.6, 6.12+)
    try:
        parts = kernel_version.strip().split(".")
        major = int(parts[0])
        minor = int(parts[1]) if len(parts) > 1 else 0
        if major > 6 or (major == 6 and minor >= 6):
            return True
    except Exception:
        pass

    return False

def get_patch_file_for_version(script_dir, kernel_version):
    """
    根据内核版本自动匹配专属补丁
    """
    ver = kernel_version.strip()
    patch_dir = os.path.join(script_dir, "../zram/patches")
    patch_dir = os.path.abspath(patch_dir)

    target_patch = None
    if ver.startswith("6.1"):
        target_patch = os.path.join(patch_dir, "zram_multi_comp_6.1.patch")
    elif ver.startswith("5.15"):
        target_patch = os.path.join(patch_dir, "zram_multi_comp_5.15.patch")
    elif ver.startswith("5.10"):
        target_patch = os.path.join(patch_dir, "zram_multi_comp_5.10.patch")

    if target_patch and os.path.isfile(target_patch):
        return target_patch
    return None

def apply_patch(kernel_root, patch_file, kernel_version):
    if not patch_file or not os.path.isfile(patch_file):
        log(f"[ERROR] 找不到内核 {kernel_version} 的 Multi-Comp 补丁文件: {patch_file}")
        return False

    zram_drv_h = os.path.join(kernel_root, "drivers/block/zram/zram_drv.h")
    if os.path.isfile(zram_drv_h):
        try:
            with open(zram_drv_h, "r", encoding="utf-8", errors="replace") as f:
                if "CONFIG_ZRAM_MULTI_COMP" in f.read():
                    log(f"[INFO] {kernel_version} 内核源码已打入过 Multi-Comp 补丁，跳过重复应用")
                    return True
        except Exception:
            pass

    patch_name = os.path.basename(patch_file)
    log(f"[INFO] 正在应用 {kernel_version} 通用内核 Multi-Comp 驱动补丁 ({patch_name})...")

    # 优先尝试 git apply
    try:
        git_cmd = ["git", "apply", "--whitespace=fix", patch_file]
        git_res = subprocess.run(git_cmd, cwd=kernel_root, capture_output=True, text=True)
        if git_res.returncode == 0:
            log(f"[OK] 通过 git apply 成功打入 {kernel_version} Multi-Comp 驱动补丁！")
            return True
    except FileNotFoundError:
        pass
    except Exception as e:
        log(f"[INFO] git apply 尝试失败: {e}")

    # 次选尝试 patch -p1
    try:
        cmd = ["patch", "-p1", "--no-backup-if-mismatch", "-F", "3", "-i", patch_file]
        res = subprocess.run(cmd, cwd=kernel_root, capture_output=True, text=True)
        if res.returncode == 0:
            log(f"[OK] 通过 patch 命令成功打入 {kernel_version} Multi-Comp 驱动补丁！")
            return True
        log(f"::error::[Multi-Comp] 补丁命令执行返回错误: {res.stderr}")
    except FileNotFoundError:
        log("[ERROR] 系统中未找到 git 或 patch 命令，无法应用补丁")
    except Exception as e:
        log(f"[ERROR] 应用补丁时发生异常: {e}")

    return False

def main():
    kernel_root = sys.argv[1] if len(sys.argv) > 1 else os.getcwd()
    kernel_version = sys.argv[2] if len(sys.argv) > 2 else ""
    enable_multi_comp = sys.argv[3] if len(sys.argv) > 3 else "false"
    patch_multi_comp = sys.argv[4] if len(sys.argv) > 4 else "false"
    config_file = sys.argv[5] if len(sys.argv) > 5 else ""

    script_dir = os.path.dirname(os.path.abspath(__file__))

    is_enable = str(enable_multi_comp).strip().lower() == "true"
    is_patch = str(patch_multi_comp).strip().lower() == "true"

    log("==========================================================")
    log("检查 ZRAM Multi-Comp (多流二次重压缩) 配置与修补条件...")
    log(f"内核大版本     : {kernel_version}")
    log(f"开启功能开关   : {is_enable}")
    log(f"开启修补开关   : {is_patch}")
    log(f"源码根目录     : {kernel_root}")
    log(f"配置文件路径   : {config_file}")
    log("==========================================================")

    # 1. 严格互锁校验：只开修补没开功能 -> 立即报错中断构建
    if is_patch and not is_enable:
        log("::error::[Multi-Comp] 开启Multi-Comp修补必须同时勾选【开启Multi-Comp(内核版本>=6.12)】，构建终止！")
        sys.exit(1)

    # 2. 两个都未开启 -> 保持原样安全退出
    if not is_enable and not is_patch:
        log("[INFO] 未启用 Multi-Comp 功能与修补，保持原样。")
        sys.exit(0)

    # 3. 检查当前内核是否原生支持
    is_native = check_native_support(kernel_root, kernel_version)

    # 4. 只开启功能，未开启修补
    if is_enable and not is_patch:
        if is_native:
            log(f"[OK] 检测到当前内核版本 ({kernel_version}) 原生支持 Multi-Comp，直接追加配置开启！")
            append_config(config_file)
            sys.exit(0)
        else:
            log(f"::warning::[Multi-Comp] 当前内核版本 ({kernel_version}) 原生不支持免补丁开启 Multi-Comp，已安全跳过开启！如需启用请同时勾选【开启Multi-Comp修补(修补不支持的版本)】！")
            sys.exit(0)

    # 5. 两个同时开启（开启功能 + 开启修补）
    if is_enable and is_patch:
        if is_native:
            log(f"[INFO] 当前内核版本 ({kernel_version}) 已原生内置 Multi-Comp 驱动支持，无需打补丁，直接追加配置开启！")
            append_config(config_file)
            sys.exit(0)
        else:
            log(f"[INFO] 当前内核版本 ({kernel_version}) 原生未包含 Multi-Comp，开始匹配对应补丁执行修补...")
            patch_file = get_patch_file_for_version(script_dir, kernel_version)
            if not patch_file:
                log(f"::error::[Multi-Comp] 未找到内核版本 ({kernel_version}) 的 Multi-Comp 适配补丁，支持版本: 5.10, 5.15, 6.1 及原生版本 6.6, 6.12+！构建终止！")
                sys.exit(1)

            success = apply_patch(kernel_root, patch_file, kernel_version)
            if not success:
                log(f"::error::[Multi-Comp] {kernel_version} Multi-Comp 源码补丁打入失败，构建终止！")
                sys.exit(1)

            append_config(config_file)
            log(f"[OK] 内核 {kernel_version} Multi-Comp 驱动补全与配置开启成功！")
            sys.exit(0)

if __name__ == "__main__":
    main()
