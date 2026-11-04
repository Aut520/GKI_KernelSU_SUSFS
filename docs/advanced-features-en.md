<div align="center">

# 🧩 Advanced Features & Deep Customization Guide

**Security Vulnerability Fixes · Anti-Bricking Guard · NoMount Meta Modules · ZRAM Multi-Comp · Droidspaces Containers · Stock Config Spoofing**

English | [**简体中文**](advanced-features.md)

[⬅ Back to Main README](../README-EN.md)

---

</div>

> [!CAUTION]
> **Testing Status & Compatibility Notice**:
> All advanced features and low-level patches detailed in this document have **ONLY been fully tested and validated on `Android 14 (6.1)` and `Android 12 (5.10)` kernel branches**.
> For other branches (`5.15`, `6.6`, `6.12`), due to structural differences and rapid symbol refactoring in upstream ACK codebases, **successful builds and proper booting are NOT guaranteed**. Please test prudently and ALWAYS create a full backup of your Boot partition beforehand!

---

## Contents

- [🛡️ GhostLock Vulnerability Remediation (CVE-2026-43499 / 53163)](#️-ghostlock-vulnerability-remediation-cve-2026-43499--53163)
- [🛑 BBG Low-Level Anti-Bricking Guard (hmbird_patch)](#-bbg-low-level-anti-bricking-guard-hmbird_patch)
- [📦 NoMount Kernel-Level Mountless Meta Modules](#-nomount-kernel-level-mountless-meta-modules)
- [⚡ Enhanced ZRAM & Multi-Compression Streams (Multi-Comp)](#-enhanced-zram--multi-compression-streams-multi-comp)
- [🌐 Kernel Networking Stack Enhancements (BBRv3 / IPSet / Qdisc)](#-kernel-networking-stack-enhancements-bbrv3--ipset--qdisc)
- [🧪 Spoofing `/proc/config.gz` (Stock Config Replacement)](#-spoofing-procconfiggz-stock-config-replacement)
- [🧪 Droidspaces Container Support (Experimental)](#-droidspaces-container-support-experimental)
- [🔧 Custom Commit Pinning](#-custom-commit-pinning)

---

## 🛡️ GhostLock Vulnerability Remediation (CVE-2026-43499 / 53163)

GhostLock represents a cluster of high-severity Linux kernel vulnerabilities tracked under `CVE-2026-43499` and `CVE-2026-53163`. Attackers do not require Root access; any unauthorized, untrusted user-space application running local code can trigger exploitation.

### Threat & Impact
- **Denial of Service (Forced Reboot / Kernel Panic):** Any unprivileged app can force a kernel crash, causing boot loops or loss of unsaved state.
- **Local Privilege Escalation (LPE):** Exploits lock contention in `rtmutex` to breach Android's application isolation barriers, elevating an unprivileged process to kernel-level control.
- **Public PoCs Available:** Functional privilege escalation exploit chains for Android ARM64 are publicly documented.

### Implementation
- **Default State**: Enabled by default (`default: true`) in this repository.
- **Automatic Validation**: The build script inspects the target kernel tree and applies the `kernel/locking/rtmutex.c` patch chain only when fixes are not already upstreamed by Google.

---

## 🛑 BBG Low-Level Anti-Bricking Guard (hmbird_patch)

Malicious modules or rogue utilities obtaining Root rights might attempt destructive operations such as `dd if=/dev/zero` or `blkdiscard` against essential storage partitions, bricking the device or corrupting baseband calibration data.

### Implementation
- Injects `hmbird_patch.c` into the kernel block I/O layer.
- Enforces an immutable write-protection list covering critical partitions (such as `boot`, `init_boot`, `recovery`, `super`, `userdata`, `metadata`, `persist`, and modem partitions).
- Destructive raw write or wipe attempts are trapped at the scheduler level and rejected.
- **Default State**: Enabled by default (`default: true`).

---

## 📦 NoMount Kernel-Level Mountless Meta Modules

Traditional root solutions rely on filesystem mount points (OverlayFS / bind mounts). These operations leave clearly visible traces inside `/proc/mounts` and `/proc/self/mountinfo`, which are easily inspected by bank apps and integrity scanners.

### Implementation
- Incorporates [maxsteeel/nomount](https://github.com/maxsteeel/nomount).
- Redirects path resolution (`vfs_lookup`) and inode evaluation directly inside the kernel VFS layer.
- Module content is substituted without issuing `mount` syscalls, leaving no mount table entries for security software to detect.
- **Default State**: Enabled by default (`default: true`).

---

## ⚡ Enhanced ZRAM & Multi-Compression Streams (Multi-Comp)

Background multitasking and overall Android fluidity heavily rely on fast memory compression. Stock kernels frequently limit ZRAM to single-stream execution or older compression algorithms.

### Implementation
- Unlocks modern ZSTD and LZ4 compression algorithms within the kernel ZRAM driver.
- Enables `Multi-Comp` concurrent stream processing across multiple CPU cores, effectively eliminating lock contention under intense memory pressure.
- Operates in synergy with Android's userland `mmd` (Memory Management Daemon) to reduce frame drops during memory compaction.
- **Default State**: Enabled by default (`default: true`).

---

## 🌐 Kernel Networking Stack Enhancements (BBRv3 / IPSet / Qdisc)

Tailored to improve stability and latency under cellular networks, proxies, and weak Wi-Fi signals:
- **BBRv3 Congestion Control**: Delivers superior throughput and lower latency over congested paths compared to legacy Cubic.
- **IPSet Packet Matching**: Accelerates route and firewall evaluation across extensive IP rulesets.
- **Modern Qdisc Schedulers**: Enables queueing disciplines such as fq_codel and cake to eliminate bufferbloat.
- **Default State**: Enabled by default (`default: true`).

---

## 🧪 Spoofing `/proc/config.gz` (Stock Config Replacement)

Many integrity detection solutions inspect `/proc/config.gz` looking for telltale build symbols like `CONFIG_KSU=y` or `CONFIG_KSU_SUSFS=y`.

### Implementation
1. The repository maintains pristine factory configuration files (`config/stock_<ver>.config`).
2. During the kernel linking stage, the internal `config_data` source embedded into the binary is substituted with the untouched stock defconfig.
3. Userspace inspection tools extracting `/proc/config.gz` will see standard OEM compilation flags without any third-party root markers.

---

## 🧪 Droidspaces Container Support (Experimental)

[Droidspaces](https://github.com/ravindu644/Droidspaces-OSS) unlocks lightweight Linux containers (such as Arch Linux or Debian with systemd) on Android utilizing native kernel namespaces and cgroups.

### Recommendations
- Select the `Droidspaces` input parameter when manually triggering the workflow.
- Try different slot patches (`678`, `123`, `345`) if compilation fails or bootloop occurs, as symbol alignments vary across kernel sub-levels.

---

## 🔧 Custom Commit Pinning

To freeze specific commit hashes for SUSFS or SukiSU (e.g. for reproducibility or bisecting upstream regressions), configure [`config/config`](../config/config):

```ini
# Enable custom commits
custom=true

# Commit hash per SUSFS branch
gki-android12-5.10=
gki-android13-5.15=
gki-android14-6.1=
gki-android15-6.6=

# SukiSU commit hash
sukisu=
```
- Leave blank to pull the latest head commit from the respective upstream branch.

---

<div align="center">

[⬅ Back to Main README](../README-EN.md) · [简体中文](advanced-features.md)

</div>
