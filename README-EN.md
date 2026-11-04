<div align="center">

# GKI KernelSU SUSFS

**Automated Android Generic Kernel Image (GKI) Build Pipeline | Integrated KernelSU + SUSFS + Security & Performance Enhancements**

[![GitHub Actions](https://img.shields.io/badge/CI-GitHub_Actions-2088FF?style=flat-square&logo=githubactions&logoColor=white)](https://github.com/Aut520/GKI_KernelSU_SUSFS/actions)
[![License](https://img.shields.io/badge/License-GPLv2-blue.svg?style=flat-square)](LICENSE)
[![KernelSU](https://img.shields.io/badge/KernelSU-Official_Default-5AA300?style=flat-square)](https://kernelsu.org/)
[![SUSFS](https://img.shields.io/badge/SUSFS-Integrated-E67E22?style=flat-square)](https://gitlab.com/simonpunk/susfs4ksu)

English | [**简体中文**](README.md)

---

</div>

## ⚠️ Critical Compatibility & Testing Status Notice

> [!CAUTION]
> **Crucial Verification Status:**
> - **The majority of features, enhancement patches, and build fixes in this repository have ONLY been thoroughly tested and verified on `Android 14 (6.1)` and `Android 12 (5.10)` kernels.**
> - For other kernel versions (such as `Android 13 (5.15)`, `Android 15 (6.6)`, and `Android 16 (6.12)`), due to extensive upstream AOSP codebase divergence, frequent symbol changes, and patch conflicts, **it is currently UNKNOWN whether they will successfully compile or boot properly without issues.**
> - If building for unverified kernel branches, be prepared to troubleshoot potential compilation issues independently, and **ALWAYS perform a complete backup of your device's `boot` / `init_boot` partitions via fastboot or custom recovery before flashing!**

---

## 📌 Upstream References & Credits

This project is built upon outstanding open-source contributions from the community:

- **Automated Workflow Foundation**: [zzh20188/GKI_KernelSU_SUSFS](https://github.com/zzh20188/GKI_KernelSU_SUSFS) (Original workflow creator)
- **Official KernelSU**: [tiann/KernelSU](https://github.com/tiann/KernelSU) (Kernel-based root solution for Android)
- **SuSFS Camouflage Framework**: [simonpunk/susfs4ksu](https://gitlab.com/simonpunk/susfs4ksu) (Kernel-level evasion & root concealment filesystem)
- **Community KernelSU Variants**:
  - [SukiX-Team/SukiSU](https://github.com/SukiX-Team/SukiSU) & [SukiSU-Ultra](https://github.com/SukiSU-Ultra/SukiSU-Ultra)
  - BakaSU Variant
- **NoMount Meta Module**: [maxsteeel/nomount](https://github.com/maxsteeel/nomount)
- **Anti-Bricking Protection (BBG)**: `hmbird_patch.c` critical block partition protection
- **Kernel Packaging Engine**: [osm0sis/AnyKernel3](https://github.com/osm0sis/AnyKernel3)
- **Container Environment**: [ravindu644/Droidspaces-OSS](https://github.com/ravindu644/Droidspaces-OSS)
- **Security Patches**: GhostLock vulnerability research team & Linux upstream rtmutex fixes

---

## 🔄 Core Changes & Enhancements in This Repository

Compared to upstream, the following modifications and adaptations have been introduced:

### 1. Removal of Upstream Hardcoded Identity & Distribution Restrictions
- **Decoupled Workflow Permissions**: Completely removed hardcoded repository owner checks in `.github/workflows/main.yml` and `Auto_Trigger.yml`. Forked repositories can now independently execute automated builds and publish Releases.
- **Resource Download Source Redirection**: Fixed hardcoded raw download URLs across `get-manager.yml` and `build.yml`. Legacy APKs, SUSFS companion modules, and `hmbird_patch.c` are now pulled directly from this repository (`Aut520/GKI_KernelSU_SUSFS`), eliminating 404 errors during build.

### 2. Out-of-the-Box Optimized Default Settings
To deliver maximum stability and security without requiring manual configuration adjustments:
- **Default KernelSU Variant**: Changed from `BakaSU` to **`Official`** (highest system stability and seamless module ecosystem compatibility).
- **Default Release Target**: Switched from ephemeral `Actions` artifacts to **`Release`** (automatically generates full GitHub Releases with all required binaries and modules attached).
- **Recommended Feature Toggles Enabled by Default**:
  - `use_bbg: true`: Enables BBG anti-wipe protection by default;
  - `use_zram: true`: Enables enhanced ZRAM multi-comp algorithms by default;
  - `use_nomount: true`: Enables kernel-level NoMount meta module support by default;
  - `use_networking: true`: Enables BBRv3, IPSet, Qdisc, and CIFS networking stack enhancements by default;
  - `cve_2026_43499_patch: true`: Enables the complete GhostLock rtmutex privilege escalation fix chain by default.

### 3. Dedicated 5.10 & 6.1 Compilation & Patch Fixes
- **Restored Thin-LTO on 5.10**: Upstream completely removed Thin-LTO. However, linking monolithic `vmlinux.o` with Full LTO on 5.10 frequently exhausts memory on GitHub Actions runners, resulting in OOM kills (SIGKILL) or timeouts. This repository restores `LTO=thin` specifically for 5.10 in `build.yml` for predictable builds.
- **Fixed `preinit.c` Missing `close_fd` on 5.10**: Patched missing header inclusions causing compilation errors in SUSFS on kernel 5.10.
- **Automatic Fallback for `fs/namespace.c`**: Enhanced `scripts/susfs_fixes/apply.sh` to automatically detect and supplement missing forward declarations when official SUSFS patches experience Hunk #1 offset failures.
- **Real-Time UTC Timestamps**: Removed obsolete hardcoded 2024 build timestamps, restoring automatic dynamic evaluation to the current UTC time.

---

## 🛠️ Features & Underlying Implementation

| Feature | Description | Underlying Mechanism |
| :--- | :--- | :--- |
| **KernelSU Variants** | Official, SukiSU, and BakaSU variant choices | Dynamic submodule ingestion into kernel tree during Bazel/Make phase, hooking into kernel syscalls. |
| **SuSFS Framework** | Advanced stealth for Root processes and custom mounts | Hooks VFS layer, overriding `openat`, `stat`, and directory traversal syscalls to conceal mount points and KSU hooks. |
| **BBG Anti-Bricking** | Defends against malicious scripts attempting to wipe partitions | Injects `hmbird_patch.c` at block device layer, filtering raw write requests to `boot`, `super`, `userdata`, and modem partitions. |
| **NoMount Meta Modules** | Injects root modules without visible user-space mount points | Based on `maxsteeel/NoMount`, directly redirects inode lookup at the kernel filesystem layer to bypass mount surveillance. |
| **ZRAM Multi-Comp** | Boosts memory throughput and reduces compaction latency | Patches ZRAM driver to support ZSTD and LZ4 with multi-compression stream channels, cooperating with Android's userland `mmd`. |
| **Network Optimization** | Enhances connection throughput and queuing efficiency | Activates BBRv3 congestion control, IPSet matching rules, advanced Qdisc schedulers, and CIFS in kernel defconfig. |
| **GhostLock Security Fix** | Mitigates local privilege escalation and denial-of-service | Resolves `CVE-2026-43499` and `CVE-2026-53163` by patching lock inheritance race conditions in `kernel/locking/rtmutex.c`. |
| **Stock Config Spoofing** | Camouflages `/proc/config.gz` against detection tools | Embeds clean stock defconfig into the compiled kernel image, hiding custom `CONFIG_KSU_*` definitions from runtime inspectors. |
| **Xiaomi 14 Pro Screen Fix** | Resolves high-kernel (>=6.1.140) display flickering | Automatically incorporates official Xiaomi open-source display patches (`vendor_opensource_display-drivers`). |

---

## 🏗️ Automated Build Pipeline

The CI pipeline runs entirely on GitHub Actions:

```text
[Trigger (main.yml / Auto_Trigger.yml)]
       │
       ▼
[Sub-Workflow Dispatch (kernel-*.yml)]
       │
       ▼
[Runner Provisioning & Disk Cleanup (40GB+ reclaimed)]
       │
       ▼
[Fetch AOSP ACK Kernel Sources (repo / gki_fetch.py)]
       │
       ▼
[Inject Features: KSU -> SuSFS -> susfs_fixes -> NoMount -> BBG -> Net/CVE]
       │
       ▼
[Merge defconfig & Apply Stock Config Spoofing]
       │
       ▼
[Compile Kernel Image via Clang / LLVM / Bazel]
       │
       ▼
[Package AnyKernel3 Flashable ZIP Archive]
       │
       ▼
[Fetch Corresponding Manager APKs & Modules (get-manager.yml)]
       │
       ▼
[Publish Official GitHub Release Assets]
```

---

## 📖 Flashing Guide

### 1. Prerequisites
- Confirm your device is based on **Android GKI (Generic Kernel Image)** architecture (typically devices shipped with Android 12+).
- Verify your current kernel version (e.g. running `uname -r` in terminal: `6.1.xxx` requires A14-6.1, `5.10.xxx` requires A12-5.10).
- **Always backup your `boot` and `init_boot` partitions first!**

### 2. Flashing Methods
- **Method 1 (Custom Recovery)**: Flash the generated `AnyKernel3-*.zip` in TWRP / OrangeFox.
- **Method 2 (Kernel Flasher)**: Flash the kernel zip package on an already rooted system using Kernel Flasher.
- **Method 3 (Fastboot Image Flash)**: Extract the kernel `Image` from the zip package and flash it to the designated boot partition via fastboot.

---

## 📄 License

Kernel patches and modifications are licensed under the [GNU General Public License v2.0](LICENSE). Upstream components belong to their respective copyright holders.
