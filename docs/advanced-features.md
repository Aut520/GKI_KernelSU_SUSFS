<div align="center">

# 🧩 进阶功能与深度定制文档

**安全漏洞修补 · 防格机机制 · NoMount 挂载 · ZRAM 多流增强 · Droidspaces 容器 · 出厂配置伪装**

[English](advanced-features-en.md) | **简体中文**

[⬅ 返回主说明文档](../README.md)

---

</div>

> [!CAUTION]
> **测试状态与兼容性提醒**：
> 本文档介绍的所有进阶功能与底层补丁，**仅在 `Android 14 (6.1)` 与 `Android 12 (5.10)` 内核分支上经过充分的编译测试与真机验证**。
> 对于 `5.15`、`6.6`、`6.12` 等其他分支，受 upstream 补丁差异与符号重构影响，**无法保证均能构建成功或正常开机**，请谨慎选择并务必提前备份 Boot 分区！

---

## 目录

- [🛡️ GhostLock 安全漏洞修补 (CVE-2026-43499 / 53163)](#️-ghostlock-安全漏洞修补-cve-2026-43499--53163)
- [🛑 BBG 底层防格机保护 (hmbird_patch)](#-bbg-底层防格机保护-hmbird_patch)
- [📦 NoMount 内核级免挂载元模块](#-nomount-内核级免挂载元模块)
- [⚡ ZRAM 算法增强与多压缩流 (Multi-Comp)](#-zram-算法增强与多压缩流-multi-comp)
- [🌐 内核网络栈增强 (BBRv3 / IPSet / Qdisc)](#-内核网络栈增强-bbrv3--ipset--qdisc)
- [🧪 伪装 `/proc/config.gz`（Stock Config 替换）](#-伪装-procconfiggzstock-config-替换)
- [🧪 Droidspaces 容器支持（实验性）](#-droidspaces-容器支持实验性)
- [🔧 自定义提交锁定 (Commit Pinning)](#-自定义提交锁定-commit-pinning)

---

## 🛡️ GhostLock 安全漏洞修补 (CVE-2026-43499 / 53163)

GhostLock 是影响 Linux 内核的一组高风险漏洞，涵盖 `CVE-2026-43499` 与 `CVE-2026-53163`。攻击者无需 Root 权限，只要在设备上运行任意未授权普通本地代码（如恶意应用），即可利用该漏洞。

### 潜在威胁与危害
- **系统强制崩溃（DoS）：** 普通应用即可触发内核 Panic，致使手机卡死或循环重启，导致未保存数据丢失。
- **本地越权提升（LPE）：** 可绕过 Android 沙箱边界，直接利用 `rtmutex` 的竞争窗口越权获取内核级执行权限。
- **现成利用链已公开：** 针对 Android ARM64 平台的完整提权 PoC 已在社区流传。

### 本仓库实现机制
本项目支持在构建时全自动校验并应用修复链（修补 `kernel/locking/rtmutex.c`）：
- **默认状态**：在本仓库的工作流配置中，该选项已设为**默认开启**（`default: true`）。
- **智能校验**：若目标内核源码中已经由 Google 原厂合入修复，补丁脚本会自动跳过，避免冲突。

---

## 🛑 BBG 底层防格机保护 (hmbird_patch)

部分恶意模块或流氓应用在获取 Root 权限后，可能尝试通过 `dd`、`blkdiscard` 或底层写操作清空手机的关键分区，导致设备彻底变砖（甚至抹除基带或死锁）。

### 本仓库实现机制
- 在内核块设备驱动层（Block I/O）注入 `hmbird_patch.c` 钩子。
- 建立关键敏感分区保护名单（如 `boot`、`init_boot`、`recovery`、`super`、`userdata`、`metadata`、`persist` 及基带通信分区）。
- 任何未经授权的清空、格式化或覆盖写请求会被内核在 I/O 调度层直接拦截并抛出错误，最大限度防止“格机”木马侵害。
- **默认状态**：本仓库中已**默认开启**（`default: true`）。

---

## 📦 NoMount 内核级免挂载元模块

传统 Root 模块加载方案必须借助 Linux 挂载点（OverlayFS / Bind mount）。这会在 `/proc/mounts`、`/proc/self/mountinfo` 中留下大量可见的挂载条目，极易被银行、政企和游戏反作弊系统检测。

### 本仓库实现机制
- 集成 [maxsteeel/nomount](https://github.com/maxsteeel/nomount) 内核补丁。
- 在内核虚拟文件系统（VFS）的路径查找（`vfs_lookup`）与 inode 解析阶段注入重定向机制。
- 模块文件可在不调用 `mount` 挂载系统调用的前提下直接替换系统目标路径，对用户态检测程序实现彻底的“无痕化”。
- **默认状态**：本仓库中已**默认开启**（`default: true`）。开启后可搭配专用的 NoMount 配套模块使用。

---

## ⚡ ZRAM 算法增强与多压缩流 (Multi-Comp)

Android 设备后台留存与流畅度极大依赖内存压缩。默认内核通常仅启用性能有限的算法或单压缩流。

### 本仓库实现机制
- 修补内核 ZRAM 驱动，解锁 ZSTD、LZ4 等现代高效压缩算法支持。
- 对支持的内核开启 `Multi-Comp`（多流并发压缩）机制，允许多个 CPU 核心并发执行内存数据块的压缩与解压，显著消除单流锁竞争（Lock Contention）。
- 与 Android 系统的 `mmd`（Memory Management Daemon）深度协作，在高负荷内存回收时降低系统掉帧率。
- **默认状态**：本仓库中已**默认开启**（`default: true`）。

---

## 🌐 内核网络栈增强 (BBRv3 / IPSet / Qdisc)

针对移动端弱网、高延迟或复杂代理网络环境，深度增强内核网络层：
- **BBRv3 拥塞控制**：提供比原版 Cubic 更好的弱网吞吐量与连接保活能力。
- **IPSet 规则匹配**：大幅提高网络数据包在大规模 IP 黑白名单下的过滤与匹配效率。
- **Qdisc 队列调度**：支持 fq_codel、cake 等现代排队规则，抑制缓冲区膨胀（Bufferbloat）。
- **默认状态**：本仓库中已**默认开启**（`default: true`）。

---

## 🧪 伪装 `/proc/config.gz`（Stock Config 替换）

不少检测机制通过读取运行中的内核配置 `/proc/config.gz`，检测是否存在 `CONFIG_KSU=y`、`CONFIG_KSU_SUSFS=y` 等特征宏。

### 使用方法与原理
1. 构建流水线内置了各版本官方出厂的 `stock_<ver>.config`（位于仓库 [`config/`](../config/) 目录）。
2. 在构建链接阶段，Makefile 中的 `config_data` 数据源会被自动替换为未修改的官方原生配置。
3. 编译生成的内核刷入系统后，用户态读取 `/proc/config.gz` 只能获取到官方出厂时的纯净选项，无法检测到任何 Root 注入标记。

---

## 🧪 Droidspaces 容器支持（实验性）

[Droidspaces](https://github.com/ravindu644/Droidspaces-OSS) 允许在 Android 环境下借助 Linux 命名空间与 cgroups 运行完整的 Linux 容器（如 Arch Linux、Debian，并支持 systemd 服务）。

### 使用建议
- 在触发构建时可手动选择 `Droidspaces 容器支持`。
- 提供槽位补丁（`678`、`123`、`345`）。由于不同子版本内核符号位不同，如遇到构建冲突或刷入开机异常，可尝试轮换不同槽位补丁测试。

---

## 🔧 自定义提交锁定 (Commit Pinning)

若需要锁定特定时间的 SUSFS 或 SukiSU 源码版本以排查兼容性问题，可在仓库 [`config/config`](../config/config) 文件中配置：

```ini
# 启用自定义提交
custom=true

# SUSFS 各分支的 commit hash
gki-android12-5.10=
gki-android13-5.15=
gki-android14-6.1=
gki-android15-6.6=

# SukiSU 的 commit hash
sukisu=
```
- 留空则默认拉取该分支最新提交。

---

<div align="center">

[⬅ 返回主说明文档](../README.md) · [English](advanced-features-en.md)

</div>
