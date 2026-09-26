# 独立轻量级内存管理守护进程 (Standalone MMD)

本项目是从 **Android 17+ 原厂内存守护进程 `mmd` (`system/memory/mmd`)** 剥离、解耦并深度魔改而来的**独立极轻量级 ZRAM 二次重压缩守护进程**。

---

## 🌟 核心改进与解耦对比

| 特性 | AOSP 原版 `mmd` (Android 17+) | 独立魔改版 `mmd` (本项目) |
| :--- | :--- | :--- |
| **系统版本要求** | 仅限原生 Android 17+ | **通用全平台** (Android 11~15+ / 任意原生/第三方 ROM) |
| **框架依赖** | 强依赖 Java `system_server`、JobScheduler、Binder IPC、StatsD | **零框架依赖**，纯 Native 原生进程，彻底脱离 Binder 与 system_server |
| **体积与资源** | ~数十 MB（含 AIDL/Binder 运行时），占用较大 | **仅 ~400 KB** 单一二进制文件，常驻后台内存 **< 1.5 MB**，CPU **0%** |
| **触发机制** | 只能等待系统 JobScheduler 在深夜低电量唤醒 | **三模合一**：自主定时守护 + 极轻量 UNIX Socket IPC + CLI 手动/脚本即时触发 |
| **核心算法保留** | 官方指数衰减冷页插值、休眠时间补偿、精准 Sysfs 追踪 | **100% 忠实保留官方原版算法** (`SuspendMonitor`、`calculate_idle_time` 等) |

---

## 🚀 运行模式与使用方法

### 1. 命令行参数

```bash
# 查看帮助
mmd --help

# 状态检测：查看当前 ZRAM 主/从压缩算法、大小、压缩率及冷页指标
mmd --status
# 或简写
mmd -s

# 单次触发维护：立即计算最佳冷页判定时长，标记 idle，并执行二次重压缩 (Recompression)
mmd --trigger
# 或简写
mmd -t

# 自主常驻守护模式（后台自主调度 + 监听轻量 Socket）
mmd --daemon &
# 或简写
mmd -d &

# 通过轻量 Socket 向后台常驻的 mmd 发送即时指令
mmd --client trigger   # 立即唤起一次重压缩维护
mmd --client status    # 获取后台实时压缩状态
mmd --client ping      # 健康心跳检测 (返回 PONG)
```

### 2. 环境变量调节 (可选)

所有参数均内置经 Google 调优的黄金默认值，亦可通过环境变量个性化覆盖：

| 环境变量 | 默认值 | 说明 |
| :--- | :--- | :--- |
| `MMD_INTERVAL` | `3600` (秒) | 自主守护循环周期（默认 1 小时检查一次） |
| `MMD_MIN_IDLE` | `7200` (秒) | 最小冷页判定冷却时间（2 小时） |
| `MMD_MAX_IDLE` | `14400` (秒) | 最大冷页判定冷却时间（4 小时） |
| `MMD_BACKOFF` | `1800` (秒) | 两次重压缩执行之间的最短退避间隔（30 分钟） |
| `MMD_THRESHOLD`| `1024` (字节)| 不可重压缩阈值（压缩后仍大于此字节才二次重压） |
| `MMD_SOCKET` | `/data/local/tmp/mmd.sock` | 轻量 UNIX Domain Socket 通信路径 |

---

## 🛠️ 编译指南 (ARM64 纯静态独立二进制)

### 推荐：纯静态编译 (无需 NDK，零外部依赖，全平台通用)

使用 Rust 内置的 `rust-lld` 静态链接 MUSL libc，生成的单文件二进制不依赖系统任何 `.so`：

```bash
# 添加 musl target (仅首次需要)
rustup target add aarch64-unknown-linux-musl

# 一键编译纯静态 ARM64 二进制 (无需安装或配置 NDK)
cargo build --target aarch64-unknown-linux-musl --release
```
编译产物位于：
`target/aarch64-unknown-linux-musl/release/mmd` (体积仅约 ~470 KiB，单文件纯静态)

---

## 📱 手机端快速部署 (Root / Termux / Magisk)

1. 推送到手机：
```bash
adb push target/aarch64-unknown-linux-musl/release/mmd /data/local/tmp/
adb shell chmod 755 /data/local/tmp/mmd
```


2. 立即单次触发测试：
```bash
adb shell /data/local/tmp/mmd -t
```

3. 以后台服务形式常驻：
```bash
adb shell "nohup /data/local/tmp/mmd -d > /data/local/tmp/mmd.log 2>&1 &"
```
