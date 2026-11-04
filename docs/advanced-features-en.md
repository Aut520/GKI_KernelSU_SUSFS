<div align="center">

# 🧩 Advanced Features & Exclusive Enhancements Guide

**ZRAM Multi-Comp Dual Engines · Standalone Rust MMD Daemon · Xiaomi 14 Pro Screen Fix · Networking & FQ Locking · 5.10 Thin LTO · Anti-Bricking Security**

English | [**简体中文**](advanced-features.md)

[⬅ Back to Main README](../README-EN.md)

---

</div>

> [!CAUTION]
> **Testing Status & Compatibility Notice**:
> All exclusive features, driver fixes, and recompression scheduling mechanisms in this document have **ONLY been fully tested and validated on `Android 14 (6.1)` and `Android 12 (5.10)` kernel branches**.
> For other branches (`5.15`, `6.6`, `6.12`), due to structural differences and rapid symbol refactoring in upstream ACK codebases, **successful builds and proper booting are NOT guaranteed**. Please test prudently and ALWAYS create a full backup of your Boot partition beforehand!

---

## Contents

- [⚡ [Exclusive] ZRAM Multi-Comp (Multi-Stream Recompression System)](#-exclusive-zram-multi-comp-multi-stream-recompression-system)
- [🦀 [Exclusive] Standalone Lightweight Rust MMD Daemon](#-exclusive-standalone-lightweight-rust-mmd-daemon)
- [📱 [Exclusive] Xiaomi 14 Pro (SM8650) High-Kernel Display Flickering Fix](#-exclusive-xiaomi-14-pro-sm8650-high-kernel-display-flickering-fix)
- [🌐 [Exclusive] Networking Stack & FQ (Fair Queueing) Scheduler Solidification](#-exclusive-networking-stack--fq-fair-queueing-scheduler-solidification)
- [🛠️ [Exclusive] Kernel 5.10 Thin-LTO Memory Guard & Build Fixes](#️-exclusive-kernel-510-thin-lto-memory-guard--build-fixes)
- [🛡️ GhostLock Security Remediation (CVE-2026-43499 / 53163)](#️-ghostlock-security-remediation-cve-2026-43499--53163)
- [🛑 BBG Low-Level Anti-Bricking Guard (hmbird_patch)](#-bbg-low-level-anti-bricking-guard-hmbird_patch)
- [📦 NoMount Kernel-Level Mountless Meta Modules](#-nomount-kernel-level-mountless-meta-modules)
- [🧪 Spoofing `/proc/config.gz` (Stock Config Replacement)](#-spoofing-procconfiggz-stock-config-replacement)
- [🧪 Droidspaces Container Support (Experimental)](#-droidspaces-container-support-experimental)
- [🔧 Custom Commit Pinning](#-custom-commit-pinning)

---

## ⚡ [Exclusive] ZRAM Multi-Comp (Multi-Stream Recompression System)

### 1. Architectural Motivation
Default Android kernels typically employ a single compression stream or a single algorithm (e.g. LZ4). Under severe memory pressure, intense spinlock contention occurs. The Multi-Comp architecture enables a primary algorithm optimized for fast write operations (LZ4) alongside a secondary algorithm offering superior compression ratios (ZSTD) for inactive cold pages. However, official Linux 5.10, 5.15, and 6.1 GKI trees lack complete native backports.

### 2. Our Implementation & Dual-Engine Strategy
Implemented in [`scripts/manage_zram_multi_comp.py`](../scripts/manage_zram_multi_comp.py):
- **Dual-Engine Precision Semantic Patcher**:
  Standard patches frequently fail when applied across different sub-levels (e.g., 6.1.140 vs 6.1.176) due to line drift. We designed a dual-engine workflow:
  1. Executes standard `git apply` for version-specific patches;
  2. If offset or conflict occurs, the system automatically falls back to an internal **Python AST & Semantic Analysis Engine**, locating and splicing code blocks at the semantic level to guarantee 100% patch success.
- **Kernel-Level Memory Safety Fixes**:
  - **Eradicated `EINVAL` Errors**: Strictly skips `ZRAM_SAME`, `ZRAM_WB`, and `ZRAM_UNDER_WB` pages in `recompress_store` to prevent invalid page dispatch crashes;
  - **Eliminated `WARN_ON_ONCE` Kernel Warnings**: Clears `INCOMPRESSIBLE` flags and priority state inside `zram_free_page`;
  - **Adaptive Capacity Clamping**: Restricts adaptive ZRAM size strictly to no more than 3/4 of physical RAM and aligns to physical pages (`PAGE_ALIGN`);
  - **Zero Compilation Warnings**: Stripped out unused variables and indentation flaws, strictly complying with kernel `-Werror`.

---

## 🦀 [Exclusive] Standalone Lightweight Rust MMD Daemon

Source code is available in [`mmd/`](../mmd/).

### 1. Why Create an Independent MMD?
Google introduced `system/memory/mmd` in Android 17+ to monitor idle states and trigger kernel recompression. However, upstream MMD is deeply coupled with Android's Java `system_server`, Binder IPC, JobScheduler, and StatsD. It cannot operate on Android 11~15 or customized AOSP/LineageOS ROMs.

### 2. Comparative Advantages
| Metric | Upstream AOSP `mmd` (Android 17+) | Standalone Rust `mmd` (This Repo) |
| :--- | :--- | :--- |
| **Compatibility** | Restricted to Android 17+ | **Universal** across Android 11~15+ and any custom ROM |
| **Framework Coupling** | Heavy Java `system_server`, Binder IPC | **Zero framework dependency**, pure native static binary |
| **Resource Overhead** | Multi-megabyte runtime, heavy footprint | **~400 KB** single binary, RAM **< 1.5 MB**, CPU **0%** |
| **Control Modes** | Triggered only by system JobScheduler | **Tri-mode**: Autonomous daemon + UNIX Socket IPC + CLI manual trigger |
| **Algorithms** | Exponential decay cold page tracking | **100% faithful reproduction** of upstream algorithms |

### 3. Usage & Deployment
```bash
# Check status, compression ratios, and memory health
mmd -s

# Manually trigger immediate cold page evaluation and recompression
mmd -t

# Run in background daemon mode
mmd -d &

# Query or instruct the daemon via lightweight UNIX Domain Socket
mmd --client trigger   # Trigger maintenance
mmd --client status    # Retrieve live stats
mmd --client ping      # Heartbeat check
```

---

## 📱 [Exclusive] Xiaomi 14 Pro (SM8650) High-Kernel Display Flickering Fix

Script: [`scripts/fix_xiaomi_shennong_display.py`](../scripts/fix_xiaomi_shennong_display.py).

### 1. Root Cause Analysis
Xiaomi 14 Pro (`shennong`, Snapdragon 8 Gen 3 / SM8650) uses a proprietary HyperOS Display Vendor module compiled against kernel `6.1.138`. When upgrading to `6.1.140+` (e.g. `6.1.176`/`177`/`182`+) ACK, upstream changes in DPU timing and removal of Vendor Hooks cause severe **high-frequency screen flickering, backlight tearing, and boot-time display dropouts**.

### 2. Comprehensive Six-Dimensional Fixes
1. **DPU Physical Encoder Commit Wait Timing** (`dpu_encoder_phys_cmd.c`): Restores safe wait sequences ensuring `ctl_start` completion when hardware controller is unstarted, eliminating Dual-DSI frame tearing;
2. **Relaxed DRM Atomic Modeset Clone Validation** (`drm_atomic_helper.c`): Replaces `-EINVAL` with a safe pass-through when vendor clones fail validation;
3. **Restored DRM Plane Dimension Alignment** (`include/drm/drm_fourcc.h`): Restores `width / info->hsub` division to prevent non-multiple pixel jitters;
4. **Reinstated Vendor Hooks**: Rebuilds removed `pm_domain` and `cpufreq` power domain callbacks;
5. **CPU Idle Timer Jitter Mitigation**: Restores wake-up protections to prevent C-State latency from delaying VSYNC;
6. **Preserved KABI Layouts**: Retains `task_struct` alignments to guarantee module binary compatibility.

---

## 🌐 [Exclusive] Networking Stack & FQ (Fair Queueing) Scheduler Solidification

- **Solidified Default Queueing Scheduler to FQ**:
  Injects `CONFIG_NET_SCH_DEFAULT=y` and `CONFIG_DEFAULT_FQ=y`. Fair Queueing (FQ) is the canonical companion designed specifically for Google BBR. FQ provides packet pacing at the microsecond level, drastically minimizing bufferbloat and cell packet retransmission bursts;
- **Kernel 5.10 Backports**:
  Automatically patches missing sysctl symbols (`proc_dou8vec_minmax`) on 5.10 to unlock full BBRv3 compatibility;
- **Integrated Stack**: Full IPSet suite (up to 65,534 entries), CAKE, FQ_CODEL, WireGuard, and CIFS.

---

## 🛠️ [Exclusive] Kernel 5.10 Thin-LTO Memory Guard & Build Fixes

- **Thin-LTO Restoration**:
  Upstream removed forced Thin-LTO. On 5.10, Full LTO links monolithic `vmlinux.o` in a single colossal context, rapidly exhausting runner memory and triggering Linux OOM Killer (exit code 137). We strictly locked `LTO=thin` for 5.10 in `build.yml` to ensure seamless CI completion.
- **Fixed Missing `close_fd` on 5.10**:
  Added forward declarations in `scripts/susfs_fixes/apply.sh` for kernel 5.10 SUSFS compilation.

---

## 🛡️ GhostLock Security Remediation (CVE-2026-43499 / 53163)

Mitigates lock contention race conditions in `kernel/locking/rtmutex.c` that enable local unprivileged apps to achieve kernel privilege escalation or trigger denial-of-service kernel panics. Enabled by default (`default: true`).

---

## 🛑 BBG Low-Level Anti-Bricking Guard (hmbird_patch)

Filters malicious block writes targeting `boot`, `super`, `userdata`, and modem partitions directly at the block layer. Enabled by default (`default: true`).

---

## 📦 NoMount Kernel-Level Mountless Meta Modules

Injects root module filesystems via direct VFS inode redirection without creating entries in `/proc/mounts`. Enabled by default (`default: true`).

---

## 🧪 Spoofing `/proc/config.gz` (Stock Config Replacement)

Substitutes the runtime config binary stream with untouched OEM stock configuration, concealing all `CONFIG_KSU_*` definitions from inspection apps.

---

## 🧪 Droidspaces Container Support (Experimental)

Enables full Linux namespaces and cgroups with slot-based kABI patches (`678`, `123`, `345`) to run containerized Linux distributions on Android.

---

## 🔧 Custom Commit Pinning

Pin specific upstream commits in [`config/config`](../config/config) to bisect regressions or maintain strict reproducibility.

---

<div align="center">

[⬅ Back to Main README](../README-EN.md) · [简体中文](advanced-features.md)

</div>
