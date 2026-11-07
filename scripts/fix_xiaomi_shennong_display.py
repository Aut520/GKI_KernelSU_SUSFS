#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
小米 14 Pro (shennong, SM8650) 全系统驱动补全与屏幕闪烁修复脚本
专用于 Android 14 6.1 高版本 (子版本 > 140，涵盖 6.1.141~6.1.182+ 全系列 LTS) GKI 内核与原厂基于 6.1.138 编译的 vendor 驱动模块协同问题：
1. 修复 DPU 物理编码器提交等待时序 (dpu_encoder_phys_cmd_wait_for_commit_done)
2. 放宽 DRM Atomic Modeset 克隆检查 (drm_atomic_check_valid_clones)
3. 还原 DRM 图层尺寸对齐计算 (drm_format_info_plane_width/height)
4. 恢复被 Google 删除的 pm_domain 与 cpufreq Vendor Hooks 声明、导出及实际调用点
5. 修复 CPU Idle 定时器唤醒防护，防止 VSYNC 抖动与背光平滑插值微闪
6. 还原 gki_defconfig 中 KABI task_struct 结构体配置
7. 自动修复 Linux 6.1.182+ LTS 内核 PM 休眠死锁缺陷 (complete -> complete_all，根除熄屏无法点亮与 AOD 失效)
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
            pattern = re.compile(
                r'if\s*\(\s*phys_enc->irq\[INTR_IDX_CTL_START\]\s*&&[\s\S]*?'
                r'return\s+_dpu_encoder_phys_cmd_wait_for_ctl_start\(phys_enc\);\s*'
                r'return\s+dpu_encoder_phys_cmd_wait_for_tx_complete\(phys_enc\);',
                re.MULTILINE
            )
            replacement = (
                "\tif (phys_enc->hw_ctl->ops.is_started(phys_enc->hw_ctl))\n"
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
                r'(failed valid clone check for mask[\s\S]*?)(return\s+-EINVAL;)',
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
        if "DIV_ROUND_UP" in content and "hsub" in content:
            new_content, c1 = re.subn(
                r'return\s+DIV_ROUND_UP\s*\(\s*width\s*,\s*info->hsub\s*\)\s*;',
                'return width / info->hsub;',
                content
            )
            if c1 > 0:
                content = new_content
                changed = True

        if "DIV_ROUND_UP" in content and "vsub" in content:
            new_content, c2 = re.subn(
                r'return\s+DIV_ROUND_UP\s*\(\s*height\s*,\s*info->vsub\s*\)\s*;',
                'return height / info->vsub;',
                content
            )
            if c2 > 0:
                content = new_content
                changed = True

        if changed:
            with open(target, "w", encoding="utf-8") as f:
                f.write(content)
            log("[OK] 已还原 DRM 图层尺寸对齐计算 (消除非整数图层采样抖动)")
        else:
            log("[INFO] drm_fourcc.h 尺寸计算已对齐或无需修改")
    except Exception as e:
        log(f"[WARN] 修复 drm_fourcc 时发生异常: {e}")

def fix_pm_domain_vendor_hooks(kernel_root):
    pm_domain_h = os.path.join(kernel_root, "include/trace/hooks/pm_domain.h")
    vendor_hooks_c = os.path.join(kernel_root, "drivers/android/vendor_hooks.c")
    domain_governor_c = os.path.join(kernel_root, "drivers/base/power/domain_governor.c")

    try:
        # 1. 确保 include/trace/hooks/pm_domain.h 声明了 android_vh_allow_domain_state
        hook_decl = (
            "\nstruct generic_pm_domain;\n"
            "DECLARE_HOOK(android_vh_allow_domain_state,\n"
            "\tTP_PROTO(struct generic_pm_domain *genpd, uint32_t idx, bool *allow),\n"
            "\tTP_ARGS(genpd, idx, allow));\n"
        )
        if not os.path.isfile(pm_domain_h):
            os.makedirs(os.path.dirname(pm_domain_h), exist_ok=True)
            pm_content = f"""/* SPDX-License-Identifier: GPL-2.0 */

#undef TRACE_SYSTEM
#define TRACE_SYSTEM pm_domain

#define TRACE_INCLUDE_PATH trace/hooks

#if !defined(_TRACE_HOOK_PM_DOMAIN_H) || defined(TRACE_HEADER_MULTI_READ)
#define _TRACE_HOOK_PM_DOMAIN_H

#include <trace/hooks/vendor_hooks.h>
{hook_decl}
#endif /* _TRACE_HOOK_PM_DOMAIN_H */

#include <trace/define_trace.h>
"""
            with open(pm_domain_h, "w", encoding="utf-8") as f:
                f.write(pm_content)
            log("[OK] 已创建 include/trace/hooks/pm_domain.h 并声明 android_vh_allow_domain_state")
        else:
            with open(pm_domain_h, "r", encoding="utf-8", errors="replace") as f:
                pm_content = f.read()
            if "android_vh_allow_domain_state" not in pm_content:
                pos = pm_content.rfind("#endif")
                if pos != -1:
                    pm_content = pm_content[:pos] + hook_decl + "\n" + pm_content[pos:]
                else:
                    pm_content += hook_decl
                with open(pm_domain_h, "w", encoding="utf-8") as f:
                    f.write(pm_content)
                log("[OK] 已向现有 include/trace/hooks/pm_domain.h 追加声明 android_vh_allow_domain_state")
            else:
                log("[INFO] include/trace/hooks/pm_domain.h 中已声明 android_vh_allow_domain_state")

        # 2. 在 drivers/android/vendor_hooks.c 导出 android_vh_allow_domain_state
        if os.path.isfile(vendor_hooks_c):
            with open(vendor_hooks_c, "r", encoding="utf-8", errors="replace") as f:
                vh_content = f.read()

            modified = False
            if "android_vh_allow_domain_state" not in vh_content:
                if "#include <trace/hooks/pm_domain.h>" not in vh_content:
                    vh_content, sub_c = re.subn(
                        r'(#include <trace/hooks/.*?>\n)',
                        r'\1#include <trace/hooks/pm_domain.h>\n',
                        vh_content,
                        count=1
                    )
                    if sub_c == 0:
                        vh_content = "#include <trace/hooks/pm_domain.h>\n" + vh_content
                vh_content += "\nEXPORT_TRACEPOINT_SYMBOL_GPL(android_vh_allow_domain_state);\n"
                modified = True

            if modified:
                with open(vendor_hooks_c, "w", encoding="utf-8") as f:
                    f.write(vh_content)
                log("[OK] 已在 vendor_hooks.c 重新导出 android_vh_allow_domain_state")
            else:
                log("[INFO] vendor_hooks.c 中已包含 android_vh_allow_domain_state 导出")

        # 3. 在 drivers/base/power/domain_governor.c 恢复真实调用点
        if os.path.isfile(domain_governor_c):
            with open(domain_governor_c, "r", encoding="utf-8", errors="replace") as f:
                dg_content = f.read()

            if "trace_android_vh_allow_domain_state" not in dg_content:
                # 注入头文件
                if "#include <trace/hooks/pm_domain.h>" not in dg_content:
                    if "#include <linux/pm_domain.h>" in dg_content:
                        dg_content = dg_content.replace("#include <linux/pm_domain.h>", "#include <linux/pm_domain.h>\n#include <trace/hooks/pm_domain.h>", 1)
                    else:
                        dg_content = "#include <trace/hooks/pm_domain.h>\n" + dg_content

                # 在 __default_power_down_ok 中 genpd 声明之后注入调用点
                target_anchor = "bool allow = true;"
                if target_anchor in dg_content:
                    # 避免重复
                    pass
                else:
                    pattern_fn = re.compile(
                        r'(static\s+bool\s+__default_power_down_ok\s*\([^)]*\)\s*\{[\s\S]*?struct\s+generic_pm_domain\s*\*genpd\s*=\s*pd_to_genpd\([^)]*\)\s*;)'
                    )
                    match = pattern_fn.search(dg_content)
                    if match:
                        inject_code = (
                            "\n\tbool allow = true;\n"
                            "\ttrace_android_vh_allow_domain_state(genpd, state, &allow);\n"
                            "\tif (!allow)\n\t\treturn false;\n"
                        )
                        pos = match.end()
                        dg_content = dg_content[:pos] + inject_code + dg_content[pos:]
                        with open(domain_governor_c, "w", encoding="utf-8") as f:
                            f.write(dg_content)
                        log("[OK] 已在 domain_governor.c 恢复 trace_android_vh_allow_domain_state 真实调用点")
    except Exception as e:
        log(f"[WARN] 补齐 PM Domain Vendor Hooks 时发生异常: {e}")

def fix_cpu_idle_tick(kernel_root):
    target = os.path.join(kernel_root, "kernel/sched/idle.c")
    if not os.path.isfile(target):
        return

    try:
        with open(target, "r", encoding="utf-8", errors="replace") as f:
            content = f.read()

        changed = False
        pattern_func = re.compile(
            r'(static\s+void\s+idle_call_stop_or_retain_tick\s*\(\s*bool\s+stop_tick\s*\)\s*\{)[\s\S]*?(\n\})',
            re.MULTILINE
        )
        if pattern_func.search(content):
            replacement = (
                r'\1\n'
                '\t(void)stop_tick;\n'
                '\t/* 保证微秒级定时器准时响应，避免背光平滑插值抖动 (补丁群4) */\n'
                '\ttick_nohz_idle_stop_tick();'
                r'\2'
            )
            content, count = pattern_func.subn(replacement, content, count=1)
            if count > 0:
                changed = True
                log("[OK] 已成功接管 idle_call_stop_or_retain_tick 函数体 (0 告警规避 -Wunused-function)")
        elif "idle_call_stop_or_retain_tick(stop_tick);" in content:
            content = content.replace(
                "idle_call_stop_or_retain_tick(stop_tick);",
                "/* 补丁群4 */ tick_nohz_idle_stop_tick();"
            )
            changed = True
            log("[OK] 已成功替换 idle_call_stop_or_retain_tick 调用点")

        if changed:
            with open(target, "w", encoding="utf-8", newline="\n") as f:
                f.write(content)
            log("[OK] CPU Idle 定时器唤醒防护修复完成")
        else:
            log("[INFO] kernel/sched/idle.c 无需修改或已具备定时器防护")
    except Exception as e:
        log(f"[WARN] 修复 CPU Idle 定时器时发生异常: {e}")

def fix_cpufreq_vendor_hooks(kernel_root):
    cpufreq_h = os.path.join(kernel_root, "include/trace/hooks/cpufreq.h")
    vendor_hooks_c = os.path.join(kernel_root, "drivers/android/vendor_hooks.c")
    freq_table_c = os.path.join(kernel_root, "drivers/cpufreq/freq_table.c")

    try:
        # 1. 确保 include/trace/hooks/cpufreq.h 声明了 android_vh_freq_table_limits
        hook_decl = (
            "\nstruct cpufreq_policy;\n"
            "DECLARE_HOOK(android_vh_freq_table_limits,\n"
            "\tTP_PROTO(struct cpufreq_policy *policy, unsigned int min_freq, unsigned int max_freq),\n"
            "\tTP_ARGS(policy, min_freq, max_freq));\n"
        )
        if not os.path.isfile(cpufreq_h):
            os.makedirs(os.path.dirname(cpufreq_h), exist_ok=True)
            cpufreq_content = f"""/* SPDX-License-Identifier: GPL-2.0 */

#undef TRACE_SYSTEM
#define TRACE_SYSTEM cpufreq

#define TRACE_INCLUDE_PATH trace/hooks

#if !defined(_TRACE_HOOK_CPUFREQ_H) || defined(TRACE_HEADER_MULTI_READ)
#define _TRACE_HOOK_CPUFREQ_H

#include <trace/hooks/vendor_hooks.h>
{hook_decl}
#endif /* _TRACE_HOOK_CPUFREQ_H */

#include <trace/define_trace.h>
"""
            with open(cpufreq_h, "w", encoding="utf-8") as f:
                f.write(cpufreq_content)
            log("[OK] 已创建 include/trace/hooks/cpufreq.h 并声明 android_vh_freq_table_limits")
        else:
            with open(cpufreq_h, "r", encoding="utf-8", errors="replace") as f:
                h_content = f.read()
            if "android_vh_freq_table_limits" not in h_content:
                pos = h_content.rfind("#endif")
                if pos != -1:
                    h_content = h_content[:pos] + hook_decl + "\n" + h_content[pos:]
                else:
                    h_content += hook_decl
                with open(cpufreq_h, "w", encoding="utf-8") as f:
                    f.write(h_content)
                log("[OK] 已向现有 include/trace/hooks/cpufreq.h 追加声明 android_vh_freq_table_limits")
            else:
                log("[INFO] include/trace/hooks/cpufreq.h 中已存在 android_vh_freq_table_limits")

        # 2. 在 drivers/android/vendor_hooks.c 导出 android_vh_freq_table_limits
        if os.path.isfile(vendor_hooks_c):
            with open(vendor_hooks_c, "r", encoding="utf-8", errors="replace") as f:
                vh_c = f.read()

            modified = False
            if "android_vh_freq_table_limits" not in vh_c:
                if "#include <trace/hooks/cpufreq.h>" not in vh_c:
                    vh_c, sub_c = re.subn(
                        r'(#include <trace/hooks/.*?>\n)',
                        r'\1#include <trace/hooks/cpufreq.h>\n',
                        vh_c,
                        count=1
                    )
                    if sub_c == 0:
                        vh_c = "#include <trace/hooks/cpufreq.h>\n" + vh_c
                vh_c += "\nEXPORT_TRACEPOINT_SYMBOL_GPL(android_vh_freq_table_limits);\n"
                modified = True
            if modified:
                with open(vendor_hooks_c, "w", encoding="utf-8") as f:
                    f.write(vh_c)
                log("[OK] 已在 vendor_hooks.c 重新导出 android_vh_freq_table_limits")
            else:
                log("[INFO] vendor_hooks.c 中已包含 android_vh_freq_table_limits 导出")

        # 3. 在 drivers/cpufreq/freq_table.c 恢复真实调用点
        if os.path.isfile(freq_table_c):
            with open(freq_table_c, "r", encoding="utf-8", errors="replace") as f:
                ft_content = f.read()

            if "trace_android_vh_freq_table_limits" not in ft_content:
                if "#include <trace/hooks/cpufreq.h>" not in ft_content:
                    if "#include <linux/cpufreq.h>" in ft_content:
                        ft_content = ft_content.replace("#include <linux/cpufreq.h>", "#include <linux/cpufreq.h>\n#include <trace/hooks/cpufreq.h>", 1)
                    else:
                        ft_content = "#include <trace/hooks/cpufreq.h>\n" + ft_content

                anchor_target = "policy->min = policy->cpuinfo.min_freq = min_freq;"
                if anchor_target in ft_content:
                    ft_content = ft_content.replace(
                        anchor_target,
                        "\ttrace_android_vh_freq_table_limits(policy, min_freq, max_freq);\n\t" + anchor_target
                    )
                    with open(freq_table_c, "w", encoding="utf-8") as f:
                        f.write(ft_content)
                    log("[OK] 已在 freq_table.c 恢复 trace_android_vh_freq_table_limits 真实调用点")
    except Exception as e:
        log(f"[WARN] 恢复 cpufreq 钩子时发生异常: {e}")

def fix_pm_sleep_completion(kernel_root):
    """
    修复 Linux 6.1.182+ LTS 引入的 PM 设备休眠完成量死锁缺陷
    (commit 2fe7ebb3d50c8: complete() 导致 Token 耗尽死锁，上游已紧急 Revert afb942a5c521 但 Google LTS 尚未合入)
    将 complete(&dev->power.completion) 还原为 complete_all(&dev->power.completion)，
    避免熄屏后无法唤醒和 AOD 冻结。
    """
    target = os.path.join(kernel_root, "drivers/base/power/main.c")
    if not os.path.isfile(target):
        return

    try:
        with open(target, "r", encoding="utf-8", errors="replace") as f:
            content = f.read()

        changed = False
        # 1. 还原 device_pm_sleep_init 中的 complete_all
        if "init_completion(&dev->power.completion);" in content and "complete(&dev->power.completion);" in content:
            content = content.replace(
                "init_completion(&dev->power.completion);\n\tcomplete(&dev->power.completion);",
                "init_completion(&dev->power.completion);\n\tcomplete_all(&dev->power.completion);"
            )
            changed = True
            log("[OK] 已还原 device_pm_sleep_init 为 complete_all (修复熄屏休眠死锁与 AOD 无反应)")

        # 2. 移除 dpm_wait 中为配合 complete 引入的多余 no_pm 特判 (对齐上游 Revert 提交 afb942a5c521)
        if "/* Devices with no PM support don't use the completion. */\n\tif (dev->power.no_pm)\n\t\treturn;\n" in content:
            content = content.replace(
                "\t/* Devices with no PM support don't use the completion. */\n\tif (dev->power.no_pm)\n\t\treturn;\n\n",
                ""
            )
            changed = True
            log("[OK] 已清理 dpm_wait 中 no_pm 特判 (对齐 Linux 官方 Revert 补丁)")

        # 3. 还原 __device_suspend_late 中的 pm_runtime_disable 行为
        if "\tpm_runtime_disable(dev);\n\n\tdpm_wait_for_subordinate(dev, async);" in content:
            content = content.replace(
                "\tpm_runtime_disable(dev);\n\n\tdpm_wait_for_subordinate(dev, async);",
                "\t__pm_runtime_disable(dev, false);\n\n\tdpm_wait_for_subordinate(dev, async);"
            )
            changed = True
            log("[OK] 已还原 __device_suspend_late 为 __pm_runtime_disable(dev, false) (避免打乱 GDSC 电源域时序)")

        if changed:
            with open(target, "w", encoding="utf-8") as f:
                f.write(content)
        else:
            log("[INFO] drivers/base/power/main.c 无需修补或已为安全状态")
    except Exception as e:
        log(f"[WARN] 修复 PM Sleep Completion 时发生异常: {e}")

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
    raw_enabled = sys.argv[2] if len(sys.argv) > 2 else "false"
    kernel_version = sys.argv[3] if len(sys.argv) > 3 else ""
    sub_level = sys.argv[4] if len(sys.argv) > 4 else ""
    os_patch_level = sys.argv[5] if len(sys.argv) > 5 else ""

    is_enabled = str(raw_enabled).strip().lower() in ("true", "1", "yes", "on")

    log("=================================================================")
    log("检查小米 14 Pro 驱动补全与屏幕闪烁修复配置 (版本规则: 6.1 > 140 直接放行)...")
    log(f"用户勾选状态: {is_enabled}")
    log(f"当前构建参数: 版本={kernel_version}, 子版本号={sub_level}, 补丁级别={os_patch_level}")
    log(f"源码目标路径: {kernel_root}")
    log("=================================================================")

    # 1. 检查是否勾选
    if not is_enabled:
        log("[INFO] 用户未启用小米14Pro驱动补全与屏幕闪烁修复，跳过。")
        sys.exit(0)

    # 2. 从源码 Makefile 解析内核真实大版本与子版本号
    actual_sublevel = sub_level
    actual_major = ""
    actual_minor = ""
    makefile_path = os.path.join(kernel_root, "Makefile")
    if os.path.isfile(makefile_path):
        try:
            with open(makefile_path, "r", encoding="utf-8", errors="replace") as mf:
                for line in mf:
                    line_strip = line.strip()
                    if line_strip.startswith("VERSION ="):
                        actual_major = line_strip.split("=")[-1].strip()
                    elif line_strip.startswith("PATCHLEVEL ="):
                        actual_minor = line_strip.split("=")[-1].strip()
                    elif line_strip.startswith("SUBLEVEL ="):
                        actual_sublevel = line_strip.split("=")[-1].strip()
        except Exception:
            pass

    major_minor = kernel_version
    if actual_major and actual_minor:
        major_minor = f"{actual_major}.{actual_minor}"

    # 3. 动态放行规则：大版本为 6.1 且子版本号 > 140（含 LTS 分支），直接放行应用，不写死任何具体版本号
    target_version_str = f"{major_minor}.{actual_sublevel}"
    is_allowed = False
    try:
        sub_num = int(actual_sublevel)
        if major_minor == "6.1" and sub_num > 140:
            is_allowed = True
    except Exception:
        if major_minor == "6.1":
            is_allowed = True

    if not is_allowed:
        log(f"[INFO] 当前内核版本为 {target_version_str}，未满足 6.1 (> 140) 放行条件，安全跳过补丁。")
        sys.exit(0)

    log(f"[OK] 当前内核版本为 {target_version_str} (满足 6.1 > 140)，直接放行，开始应用完整修复...")

    # 4. 执行全系统级关键补丁群修复
    fix_dpu_encoder_phys_cmd(kernel_root)
    fix_drm_atomic_helper(kernel_root)
    fix_drm_fourcc(kernel_root)
    fix_pm_domain_vendor_hooks(kernel_root)
    fix_cpufreq_vendor_hooks(kernel_root)
    fix_cpu_idle_tick(kernel_root)
    fix_pm_sleep_completion(kernel_root)
    fix_gki_defconfig(kernel_root)

    log(f"[DONE] 小米 14 Pro ({target_version_str}) 驱动补全与屏幕闪烁修复全部处理完成！")

if __name__ == "__main__":
    main()
