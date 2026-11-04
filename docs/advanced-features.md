<div align="center">

# 🧩 进阶功能与独家增强补丁全景文档

**ZRAM Multi-Comp 双保险引擎 · 独立 Rust MMD 守护进程 · 小米 14 Pro 屏幕修复 · 网络增强与 FQ 固化 · 5.10 Thin LTO · 安全与防格机**

[English](advanced-features-en.md) | **简体中文**

[⬅ 返回主说明文档](../README.md)

---

</div>

> [!CAUTION]
> **测试状态与兼容性提醒**：
> 本文档介绍的所有进阶功能、底层驱动修复与重压缩调度体系，**仅在 `Android 14 (6.1)` 与 `Android 12 (5.10)` 内核分支上经过充分的编译测试与真机运行验证**。
> 对于 `5.15`、`6.6`、`6.12` 等其他分支，受 upstream 补丁差异与符号重构影响，**无法保证均能构建成功或正常开机**，请谨慎选择并务必提前备份 Boot 分区！

---

## 目录

- [⚡ 【独家】ZRAM Multi-Comp 多流二次重压缩系统](#-独家zram-multi-comp-多流二次重压缩系统)
- [🦀 【独家】独立轻量级 Rust MMD 内存守护进程](#-独家独立轻量级-rust-mmd-内存守护进程)
- [📱 【独家】小米 14 Pro (SM8650) 高内核屏幕闪烁驱动全系统修复](#-独家小米-14-pro-sm8650-高内核屏幕闪烁驱动全系统修复)
- [🌐 【独家】网络增强与 FQ (Fair Queueing) 队列调度器固化](#-独家网络增强与-fq-fair-queueing-队列调度器固化)
- [🛠️ 【独家】5.10 内核 Thin LTO 内存保护与编译修复](#️-独家510-内核-thin-lto-内存保护与编译修复)
- [🛡️ GhostLock 安全漏洞修补 (CVE-2026-43499 / 53163)](#️-ghostlock-安全漏洞修补-cve-2026-43499--53163)
- [🛑 BBG 底层基带与系统防格机保护 (Baseband-guard)](#-bbg-底层基带与系统防格机保护-baseband-guard)
- [🧩 一加 8E 处理器支持驱动 (hmbird_patch)](#-一加-8e-处理器支持驱动-hmbird_patch)
- [📦 NoMount 内核级免挂载元模块](#-nomount-内核级免挂载元模块)
- [🧪 伪装 `/proc/config.gz`（Stock Config 替换）](#-伪装-procconfiggzstock-config-替换)
- [🧪 Droidspaces 容器支持（实验性）](#-droidspaces-容器支持实验性)
- [🔧 自定义提交锁定 (Commit Pinning)](#-自定义提交锁定-commit-pinning)

---

## ⚡ 【独家】ZRAM Multi-Comp 多流二次重压缩系统

### 1. 技术背景
Android 原生内核通常仅配置单压缩流或单一算法（如 LZ4），在内存剧烈波动时容易出现单流自旋锁竞争（Lock Contention）。而在高版本 Linux 内核中引入的 Multi-Comp 机制，允许配置主算法（兼顾压缩速度，如 LZ4）与次算法（兼顾超高压缩比，如 ZSTD）。但在 Linux 5.10、5.15 和 6.1 等主力 GKI 内核中，原生并未完整移植此机制。

### 2. 本仓库的架构实现
本工程在 `scripts/manage_zram_multi_comp.py` 中实现了**全内核版本兼容框架**：
- **【双保险】语义修补引擎**：
  常规补丁在遇到不同子版本（如 6.1.140 到 6.1.176）源码行号位移时极易失败。本项目采用双保险设计：
  1. 第一阶段尝试标准 `git apply` 打入版本专属补丁；
  2. 若发生冲突，自动无缝切换到 **Python 源码语义分析修补引擎**，在 AST 与语义块维度精准定位函数钩子，实现 100% 成功补全。
- **内核级内存安全与缺陷排查修复**：
  - **根除 `EINVAL` 报错**：在内核 `recompress_store` 中严格过滤并跳过 `ZRAM_SAME`（全同页）、`ZRAM_WB`（已写回外存页）及 `ZRAM_UNDER_WB`（正在写回中）页面，避免向重压接口写入无效页面而崩溃。
  - **消除 `WARN_ON_ONCE` 警告**：在 `zram_free_page` 释放逻辑中，自动清理 `INCOMPRESSIBLE`（不可压缩）标记与多流优先级标记，防止内核抛出断言警告。
  - **智能容量上限约束（Clamp）**：限制 ZRAM 自适应最大容量严格不超过物理内存的 3/4 并执行物理页对齐（`PAGE_ALIGN`），防止极端高压下系统发生死锁。
  - **严格编译清理**：彻底消除了 Multi-Comp 补丁中的未使用变量（Unused variable）、未使用标签及制表符缩进隐患，满足内核 `-Werror` 零容忍编译要求。

---

## 🦀 【独家】独立轻量级 Rust MMD 内存守护进程

项目源码位于仓库 [`mmd/`](../mmd/) 目录。

### 1. 为什么要重新设计独立 MMD？
AOSP 原厂在 Android 17+ 引入了 `system/memory/mmd` 守护进程，用于在用户态智能检测系统休眠并向内核发起 ZRAM 二次重压缩。但原版强依赖 Android Java `system_server`、Binder IPC、JobScheduler 和 StatsD，不仅占用体积大（数十 MB），且无法运行在 Android 11~15 或原生/第三方 ROM 环境中。

### 2. 独立魔改版 MMD 特性对比
| 特性维度 | AOSP 原版 `mmd` (Android 17+) | 独立魔改版 `mmd` (本项目) |
| :--- | :--- | :--- |
| **系统兼容** | 仅限原生 Android 17+ | **全 Android 版本通用** (Android 11~15+ 及任意 ROM) |
| **框架依赖** | 强依赖 Java 服务、Binder IPC、StatsD | **零框架依赖**，纯 Native 纯静态二进制 |
| **体积与资源** | 数十 MB 运行时，内存开销大 | **仅 ~400 KB** 单文件，内存常驻 **< 1.5 MB**，CPU **0%** |
| **通信与控制** | 仅由系统 JobScheduler 私有唤醒 | **三模合一**：后台定时守护 + UNIX Domain Socket IPC + CLI 即时触发 |
| **核心算法** | 指数衰减冷页插值、休眠时间补偿 | **100% 忠实保留官方原版算法** (`SuspendMonitor`) |

### 3. 使用与手机端部署
```bash
# 查看状态诊断与压缩率统计
mmd -s

# 单次立即触发智能冷页计算与二次重压
mmd -t

# 以常驻后台守护服务模式运行
mmd -d &

# 通过轻量 Socket 向后台常驻实例下发指令
mmd --client trigger   # 立即唤起维护
mmd --client status    # 查询运行状态
mmd --client ping      # 健康心跳检测
```

---

## 📱 【独家】小米 14 Pro (SM8650) 高内核屏幕闪烁驱动全系统修复

驱动修复脚本位于 [`scripts/fix_xiaomi_shennong_display.py`](../scripts/fix_xiaomi_shennong_display.py)。

### 1. 故障根因分析
小米 14 Pro（`shennong`）搭载高通骁龙 8 Gen 3（SM8650）处理器。原厂澎湃 OS 的闭源 Display Vendor 模块基于内核 `6.1.138` 编译。当使用通用构建流水线将内核升级至高版本（如 `6.1.176`）时，Google ACK 对 DRM/DPU 架构进行了多项时序变动并移除了关键 Vendor Hooks，导致设备出现**高频屏幕微闪、背光插值撕裂、双 DSI 命令模式丢帧甚至开机黑屏**。

### 2. 独家六大修复维度与实现在线机制
- **版本写死限定（6.1.176）**：工作流提供布尔开关 `fix_mi14p_flicker`（默认关闭）。为确保系统绝对稳定与可预测，**版本写死限定为严格匹配 6.1.176**，不再使用下拉菜单与自动模糊猜测。若当前编译内核不是 6.1.176，脚本自动安全跳过并打印说明；
- **补全 Vendor Hook 真实调用点**：不仅在头文件中还原声明，更在 `domain_governor.c` 中真实植入 `trace_android_vh_allow_domain_state` 调用，在 `freq_table.c` 中真实植入 `trace_android_vh_freq_table_limits` 调用，让高通 Display 闭源驱动的回调能被真实触发；
- **DPU 物理编码器提交等待时序修复**（`dpu_encoder_phys_cmd.c`）：还原为安全的等待逻辑：只要硬件控制器未 `is_started`，强制等待 `ctl_start` 信号完成，彻底消除双 DSI 命令模式下的帧撕裂闪烁；
- **放宽 DRM Atomic Modeset 克隆检查**（`drm_atomic_helper.c`）：将 `drm_atomic_check_valid_clones` 失败时的 `-EINVAL` 返回放宽为安全放行，防止 vendor 驱动在图层 Modeset 时发生连锁丢帧；
- **还原 DRM 图层尺寸对齐计算**（`include/drm/drm_fourcc.h`）：还原 `width / info->hsub` 舍入逻辑，防止非整数倍图层渲染出现单像素抖动；
- **CPU Idle 定时器唤醒防护与 KABI 结构对齐**：接管 CPU 睡眠防抖，防止 CPU 进入过深 C-State 导致 VSYNC 中断延迟拉长引发背光微闪，并保持 `task_struct` 结构体布局严格对齐。

---

## 🌐 【独家】网络增强与 FQ (Fair Queueing) 队列调度器固化

本项目在内核网络栈进行了深度定制（`use_networking`）：
- **固化默认队列调度器为 FQ**：
  在内核 defconfig 中注入 `CONFIG_NET_SCH_DEFAULT=y` 与 `CONFIG_DEFAULT_FQ=y`。FQ (Fair Queueing) 是 Google 官方设计 BBR 拥塞控制算法时的**绝对黄金搭档**。FQ 能够实现毫秒级数据包无缝起搏（Pacing），极大降低重传率，根除移动端蜂窝网络下的抖动与突发丢包；
- **5.10 内核特别移植**：
  针对 5.10 内核缺少高版本 sysctl API（`proc_dou8vec_minmax`）的问题，构建流水线自动打入前置补丁，确保 5.10 也能无缝享受 BBRv3 的提速效果；
- **全套网络协议栈**：集成 IPSet（最大条目 65534）、CAKE/FQ_CODEL、TTL/HopLimit 防热点检测与 CIFS 网络文件系统。

---

## 🛠️ 【独家】5.10 内核 Thin LTO 内存保护与编译修复

- **恢复 Thin LTO**：
  上游更新移除了 Thin LTO 强制启用。在 5.10 内核中，Full LTO 链接阶段会将全部目标文件合并为一个庞大的上下文，其峰值内存消耗极易击穿 GitHub Actions Runner 限制，导致编译器被 Linux OOM Killer 直接杀掉（SIGKILL 报错 137）。本工程在 `build.yml` 中针对 5.10 内核单独恢复并锁定了 `LTO=thin`，保障编译稳定可靠。
- **修复 5.10 `preinit.c` 缺少 `close_fd`**：
  在 `scripts/susfs_fixes/apply.sh` 中针对 5.10 缺少头文件的问题增加自动补全，避免在 SUSFS 初期初始化阶段阻断编译。

---

## 🛡️ GhostLock 安全漏洞修补 (CVE-2026-43499 / 53163)

GhostLock 漏洞涵盖 `CVE-2026-43499` 与 `CVE-2026-53163`。攻击者只需运行任意未经授权的本地应用即可利用该漏洞。

### 危害与防范机制
- **系统死机与循环重启（DoS）：** 普通应用即可触发内核 Panic；
- **本地越权提权（LPE）：** 利用 `rtmutex` 的锁争用竞态条件，直接绕过 Android 沙箱提权至内核 Root 权限。
- **本仓库实现**：工作流配置中已设为**默认开启**（`default: true`），自动从 `security_patch/` 调用专用修补脚本精准打入修复，若源码中已有官方补丁则自动智能跳过。

---

## 🛑 BBG 底层基带与系统防格机保护 (Baseband-guard)

部分流氓模块或勒索病毒在获取 Root 后会调用底层命令清空底层关键分区。
- **工作原理**：集成开源项目 [vc-teahouse/Baseband-guard](https://github.com/vc-teahouse/Baseband-guard)，在 Linux 安全模块（LSM）层建立白名单安全防线。当有恶意 root 脚本或流氓模块企图通过 `dd`、`blkdiscard`、`format` 或原始 I/O 擦写抹除 `boot`、`super`、`userdata`、`metadata`、`persist`、基带及 EFS 分区时，内核安全子系统直接进行拦截并拒绝执行，杜绝手机彻底变砖格机的风险。
- **默认状态**：本仓库已设为**默认开启**（`default: true`）。

---

## 🧩 一加 8E 处理器支持驱动 (hmbird_patch)

- **工作原理**：通过工作流选项 `supp_op`（默认关闭），向内核源码树注入 `hmbird_patch.c` 并加入构建，提供特定 SoC / 一加 8E 处理器的驱动与符号支持。

---

## 📦 NoMount 内核级免挂载元模块

- **工作原理**：集成 [maxsteeel/nomount](https://github.com/maxsteeel/nomount)，在内核 VFS 文件系统的 inode 解析阶段直接完成重定向，在不产生任何 `/proc/mounts` 挂载条目的前提下加载模块，杜绝一切基于挂载点的风控检测。
- **默认状态**：本仓库已设为**默认开启**（`default: true`）。

---

## 🧪 伪装 `/proc/config.gz`（Stock Config 替换）

- **工作原理**：构建流程优先匹配 `config/stock_<ver>.config`，或从 Google 官方预编译内核提取纯净的出厂配置，在生成内核镜像时替换 `kernel/Makefile` 中的 `config_data` 数据源。刷入后无论如何读取 `/proc/config.gz`，均显示为官方原厂未修改配置，隐藏一切 Root 与定制编译宏。

---

## 🧪 Droidspaces 容器支持（实验性）

- **工作原理**：打入 SYSVIPC kABI 与 POSIX_MQUEUE 补丁，为 6.12 补齐 `init_ipc_ns` / `put_ipc_ns` 符号导出，启用完整命名空间与 cgroups。可按需选择 `678`、`123` 或 `345` 槽位补丁以适配不同内核符号。

---

## 🔧 自定义提交锁定 (Commit Pinning)

可在 [`config/config`](../config/config) 中自定义锁定指定 commit，回退稳定版本：
```ini
custom=true
gki-android12-5.10=
gki-android14-6.1=
sukisu=
```

---

<div align="center">

[⬅ 返回主说明文档](../README.md) · [English](advanced-features-en.md)

</div>
