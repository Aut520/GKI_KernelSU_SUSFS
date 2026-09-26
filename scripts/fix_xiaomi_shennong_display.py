#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
小米 14 Pro (shennong, SM8650) 全系统驱动补全与屏幕闪烁修复脚本
针对 Android 14 6.1.176 (2026-09) GKI 内核与原厂基于 6.1.138 编译的 vendor 驱动模块协同问题：
1. 修复 DPU 物理编码器提交等待时序 (dpu_encoder_phys_cmd_wait_for_commit_done)
2. 放宽 DRM Atomic Modeset 克隆检查 (drm_atomic_check_valid_clones)
3. 还原 DRM 图层尺寸对齐计算 (drm_format_info_plane_width/height)
4. 恢复被 Google 删除的 pm_domain 与 cpufreq Vendor Hooks 供电域与总线调度
5. 还原 gki_defconfig 中 KABI task_struct 结构体配置
"""

import sys
import os
import re

def log(msg):
    try:
        print(f"[Xiaomi-14Pro-Fix] {msg}", flush=True)
    except Exception:
        clean = msg.encode("ascii", errors="replace").decode("ascii")
        print(f"[Xiaomi-14Pro-Fix] {clean}", flush=True)

def fix_dpu_encoder_phys_cmd(kernel_root):
    target = os.path.join(kernel_root, "drivers/gpu/drm/msm/disp/dpu1/dpu_encoder_phys_cmd.c")
    if not os.path.isfile(target):
        log(f"[INFO] 未找到 {target}，跳过 DPU 物理编码器修复")
        return

    try:
        with open(target, "r", encoding="utf-8", errors="replace") as f:
            content = f.read()

        if "INTR_IDX_CTL_START" in content and "_dpu_encoder_phys_cmd_wait_for_ctl_start" in content:
            # 还原为 2025-06 (6.1.138) 的安全时序逻辑：
            # 无论是否有单独的 INTR_IDX_CTL_START，只要未 started 均等待 ctl_start，避免命令模式双 DSI 帧撕裂
            pattern = re.compile(
                r'if\s*\(\s*phys_enc->irq\[INTR_IDX_CTL_START\][\s\S]*?return\s+dpu_encoder_phys_cmd_wait_for_tx_complete\(phys_enc\);',
                re.MULTILINE
            )
            replacement = (
                "if (phys_enc->hw_ctl->ops.is_started(phys_enc->hw_ctl))\n"
                "\t\treturn dpu_encoder_phys_cmd_wait_for_tx_complete(phys_enc);\n\n"
                "\treturn _dpu_encoder_phys_cmd_wait_for_ctl_start(phys_enc);"
            )
            new_content, count = pattern.subn(replacement, content)
            if count > 0:
                with open(target, "w", encoding="utf-8") as f:
                    f.write(new_content)
                log("[OK] 已成功修复 dpu_encoder_phys_cmd 提交等待时序 (消除 DSI 帧撕裂闪烁)")
            else:
                log("[INFO] dpu_encoder_phys_cmd 已经是安全时序或代码无需更改")
        else:
            log("[INFO] dpu_encoder_phys_cmd 未检测到问题代码，保持原样")
    except Exception as e:
        log(f"[WARN] 修复 dpu_encoder_phys_cmd 时发生异常: {e}")

def fix_drm_atomic_helper(kernel_root):
    target = os.path.join(kernel_root, "drivers/gpu/drm/drm_atomic_helper.c")
    if not os.path.isfile(target):
        return

    try:
        with open(target, "r", encoding="utf-8", errors="replace") as f:
            content = f.read()

        if "failed valid clone check for mask" in content:
            pattern = re.compile(
                r'(failed valid clone check for mask 0x%x\\n[\s\S]*?)(return\s+-EINVAL;)',
                re.MULTILINE
            )
            new_content, count = pattern.subn(r'\1return 0; /* 放宽克隆检测以兼容 vendor 驱动 */', content)
            if count > 0:
                with open(target, "w", encoding="utf-8") as f:
                    f.write(new_content)
                log("[OK] 已成功放宽 drm_atomic_check_valid_clones 检查 (避免 Modeset 丢帧闪烁)")
            else:
                log("[INFO] drm_atomic_check_valid_clones 已被放宽或未命中")
    except Exception as e:
        log(f"[WARN] 修复 drm_atomic_helper 时发生异常: {e}")

def fix_drm_fourcc(kernel_root):
    target = os.path.join(kernel_root, "include/drm/drm_fourcc.h")
    if not os.path.isfile(target):
        return

    try:
        with open(target, "r", encoding="utf-8", errors="replace") as f:
            content = f.read()

        changed = False
        if "DIV_ROUND_UP(width, info->hsub)" in content:
            content = content.replace("return DIV_ROUND_UP(width, info->hsub);", "return width / info->hsub;")
            changed = True
        if "DIV_ROUND_UP(height, info->vsub)" in content:
            content = content.replace("return DIV_ROUND_UP(height, info->vsub);", "return height / info->vsub;")
            changed = True

        if changed:
            with open(target, "w", encoding="utf-8") as f:
                f.write(content)
            log("[OK] 已还原 drm_fourcc 尺寸向下取整计算 (避免 DPU SSPP 偶数硬件对齐越界)")
        else:
            log("[INFO] drm_fourcc 已经是基准对齐计算")
    except Exception as e:
        log(f"[WARN] 修复 drm_fourcc 时发生异常: {e}")

def fix_pm_domain_vendor_hooks(kernel_root):
    pm_domain_h = os.path.join(kernel_root, "include/trace/hooks/pm_domain.h")
    vendor_hooks_c = os.path.join(kernel_root, "drivers/android/vendor_hooks.c")

    try:
        # 1. 如果缺失 include/trace/hooks/pm_domain.h，重新创建它
        if not os.path.isfile(pm_domain_h):
            os.makedirs(os.path.dirname(pm_domain_h), exist_ok=True)
            pm_content = """/* SPDX-License-Identifier: GPL-2.0 */

#undef TRACE_SYSTEM
#define TRACE_SYSTEM pm_domain

#define TRACE_INCLUDE_PATH trace/hooks

#if !defined(_TRACE_HOOK_PM_DOMAIN_H) || defined(TRACE_HEADER_MULTI_READ)
#define _TRACE_HOOK_PM_DOMAIN_H

#include <trace/hooks/vendor_hooks.h>

struct generic_pm_domain;
DECLARE_HOOK(android_vh_allow_domain_state,
	TP_PROTO(struct generic_pm_domain *genpd, uint32_t idx, bool *allow),
	TP_ARGS(genpd, idx, allow))

#endif /* _TRACE_HOOK_PM_DOMAIN_H */

#include <trace/define_trace.h>
"""
            with open(pm_domain_h, "w", encoding="utf-8") as f:
                f.write(pm_content)
            log("[OK] 已补齐缺失的 include/trace/hooks/pm_domain.h")
        else:
            log("[INFO] include/trace/hooks/pm_domain.h 已存在")

        # 2. 在 drivers/android/vendor_hooks.c 导出 android_vh_allow_domain_state
        if os.path.isfile(vendor_hooks_c):
            with open(vendor_hooks_c, "r", encoding="utf-8", errors="replace") as f:
                vh_content = f.read()

            modified = False
            if "android_vh_allow_domain_state" not in vh_content:
                if "#include <trace/hooks/pm_domain.h>" not in vh_content:
                    vh_content = re.sub(
                        r'(#include <trace/hooks/.*?>\n)',
                        r'\1#include <trace/hooks/pm_domain.h>\n',
                        vh_content,
                        count=1
                    )
                vh_content += "\nEXPORT_TRACEPOINT_SYMBOL_GPL(android_vh_allow_domain_state);\n"
                modified = True

            if modified:
                with open(vendor_hooks_c, "w", encoding="utf-8") as f:
                    f.write(vh_content)
                log("[OK] 已在 vendor_hooks.c 重新导出 android_vh_allow_domain_state (防止显示供电域掉压)")
            else:
                log("[INFO] vendor_hooks.c 中已包含 android_vh_allow_domain_state 导出")
    except Exception as e:
        log(f"[WARN] 补齐 PM Domain Vendor Hooks 时发生异常: {e}")

def fix_cpufreq_vendor_hooks(kernel_root):
    cpufreq_h = os.path.join(kernel_root, "include/trace/hooks/cpufreq.h")
    vendor_hooks_c = os.path.join(kernel_root, "drivers/android/vendor_hooks.c")

    if not os.path.isfile(cpufreq_h):
        return

    try:
        with open(cpufreq_h, "r", encoding="utf-8", errors="replace") as f:
            content = f.read()

        if "android_vh_freq_table_limits" not in content:
            hook_decl = """
DECLARE_HOOK(android_vh_freq_table_limits,
	TP_PROTO(struct cpufreq_policy *policy, unsigned int min_freq,
		 unsigned int max_freq),
	TP_ARGS(policy, min_freq, max_freq));
"""
            content = content.replace("#endif /* _TRACE_HOOK_CPUFREQ_H */", hook_decl + "\n#endif /* _TRACE_HOOK_CPUFREQ_H */")
            with open(cpufreq_h, "w", encoding="utf-8") as f:
                f.write(content)

            if os.path.isfile(vendor_hooks_c):
                with open(vendor_hooks_c, "r", encoding="utf-8", errors="replace") as f:
                    vh_c = f.read()
                if "android_vh_freq_table_limits" not in vh_c:
                    vh_c += "\nEXPORT_TRACEPOINT_SYMBOL_GPL(android_vh_freq_table_limits);\n"
                    with open(vendor_hooks_c, "w", encoding="utf-8") as f:
                        f.write(vh_c)
            log("[OK] 已恢复 android_vh_freq_table_limits 钩子 (保证总线调度器正常调频)")
        else:
            log("[INFO] cpufreq.h 已包含 android_vh_freq_table_limits")
    except Exception as e:
        log(f"[WARN] 恢复 cpufreq 钩子时发生异常: {e}")

def fix_gki_defconfig(kernel_root):
    defconfig = os.path.join(kernel_root, "arch/arm64/configs/gki_defconfig")
    if not os.path.isfile(defconfig):
        return

    try:
        with open(defconfig, "r", encoding="utf-8", errors="replace") as f:
            lines = f.readlines()

        new_lines = [l for l in lines if not l.startswith("CONFIG_GKI_TASK_STRUCT_VENDOR_SIZE_MAX=")]
        if len(new_lines) != len(lines):
            with open(defconfig, "w", encoding="utf-8") as f:
                f.writelines(new_lines)
            log("[OK] 已重置 CONFIG_GKI_TASK_STRUCT_VENDOR_SIZE_MAX 为基线设置 (对齐出厂模块 KABI)")
        else:
            log("[INFO] gki_defconfig 中无需重置 task_struct 尺寸")
    except Exception as e:
        log(f"[WARN] 调整 gki_defconfig 时发生异常: {e}")

def main():
    kernel_root = sys.argv[1] if len(sys.argv) > 1 else os.getcwd()
    target_choice = sys.argv[2] if len(sys.argv) > 2 else "无"
    kernel_version = sys.argv[3] if len(sys.argv) > 3 else ""
    sub_level = sys.argv[4] if len(sys.argv) > 4 else ""
    os_patch_level = sys.argv[5] if len(sys.argv) > 5 else ""

    log("=================================================================")
    log("检查小米 14 Pro 全系统驱动补全与屏幕闪烁修复配置...")
    log(f"用户下拉选择: '{target_choice}'")
    log(f"当前内核参数: 版本={kernel_version}, 子版本号={sub_level}, 补丁级别={os_patch_level}")
    log(f"源码目标路径: {kernel_root}")
    log("=================================================================")

    # 1. 检查用户选择
    if target_choice == "无" or not target_choice.strip():
        log("[INFO] 用户选择了 '无'，跳过小米14Pro驱动补全与屏幕闪烁修复。")
        sys.exit(0)

    # 2. 如果用户选择了 6.1.176
    if "6.1.176" in target_choice:
        # 严格验证当前内核版本是否是 6.1.176
        # 检查方式：
        # - 如果 Makefile 存在，直接解析 SUBLEVEL
        actual_sublevel = sub_level
        makefile_path = os.path.join(kernel_root, "Makefile")
        if os.path.isfile(makefile_path):
            try:
                with open(makefile_path, "r", encoding="utf-8", errors="replace") as mf:
                    for line in mf:
                        if line.startswith("SUBLEVEL ="):
                            actual_sublevel = line.split("=")[-1].strip()
                            break
            except Exception:
                pass

        is_version_match = False
        if kernel_version == "6.1" and actual_sublevel == "176":
            is_version_match = True
        elif os_patch_level == "2026-09" and kernel_version == "6.1":
            is_version_match = True
        elif actual_sublevel == "176":
            is_version_match = True

        if not is_version_match:
            log(f"[WARN] 当前构建的内核为 {kernel_version}.{actual_sublevel} ({os_patch_level})，与所选补丁版本 (6.1.176) 不匹配！")
            log("[WARN] 为避免破坏其他内核版本的兼容性与构建，安全跳过此修补。")
            sys.exit(0)

        log("[INFO] 目标版本匹配成功 (Android 14 6.1.176 / 2026-09)，开始应用修复...")
    else:
        log(f"[WARN] 未知的修补选项: '{target_choice}'，安全跳过。")
        sys.exit(0)

    # 执行针对 6.1.176 的完整修复
    fix_dpu_encoder_phys_cmd(kernel_root)
    fix_drm_atomic_helper(kernel_root)
    fix_drm_fourcc(kernel_root)
    fix_pm_domain_vendor_hooks(kernel_root)
    fix_cpufreq_vendor_hooks(kernel_root)
    fix_gki_defconfig(kernel_root)

    log("[DONE] 小米 14 Pro (6.1.176) 驱动补全与屏幕闪烁修复组处理完成！")

if __name__ == "__main__":
    main()
