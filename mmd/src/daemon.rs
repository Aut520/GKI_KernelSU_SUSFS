// Daemon runtime with autonomous loop and lightweight UNIX Domain Socket IPC
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use log::{error, info, warn};
use mmd::suspend_history::SuspendMonitor;
use mmd::time::TimeApiImpl;
use mmd::zram::stats::ZramMmStat;
use mmd::zram::SysfsZramApi;

use crate::config::MmdConfig;
use crate::maintenance::ZramContext;

const MAX_CONCURRENT_IPC_CLIENTS: usize = 16;
static ACTIVE_IPC_CLIENTS: AtomicUsize = AtomicUsize::new(0);

pub struct MmdDaemon {
    config: MmdConfig,
    context: Arc<Mutex<ZramContext>>,
}

impl MmdDaemon {
    pub fn new(config: MmdConfig, context: Arc<Mutex<ZramContext>>) -> Self {
        Self { config, context }
    }

    pub fn run(self) -> anyhow::Result<()> {
        info!("Starting standalone mmd autonomous daemon...");
        info!(
            "Configuration: interval={}s, min_idle={}s, max_idle={}s, backoff={}s, threshold={}B",
            self.config.interval_seconds,
            self.config.min_idle_seconds,
            self.config.max_idle_seconds,
            self.config.backoff_seconds,
            self.config.threshold_bytes
        );

        // 1. Spawn Suspend Monitor Thread (15-min resolution for sleep tracking)
        let ctx_clone = self.context.clone();
        let max_idle_duration = Duration::from_secs(self.config.max_idle_seconds);
        thread::spawn(move || {
            let mut suspend_monitor = SuspendMonitor::<TimeApiImpl>::new();
            loop {
                thread::sleep(Duration::from_secs(900));
                let (suspend_duration, now_boot) = suspend_monitor.generate_suspend_duration();
                if let Ok(mut ctx) = ctx_clone.lock() {
                    ctx.suspend_history.record_suspend_duration(
                        suspend_duration,
                        now_boot,
                        max_idle_duration,
                    );
                    if suspend_duration > Duration::from_millis(100) {
                        info!(
                            "Recorded non-zero suspend duration: {:?}, boot_time: {:?}",
                            suspend_duration, now_boot
                        );
                    }
                }
            }
        });

        // 2. Spawn UNIX Domain Socket IPC Server (Non-blocking, minimal overhead)
        let ctx_ipc = self.context.clone();
        let cfg_ipc = self.config.clone();
        thread::spawn(move || {
            if let Err(e) = run_socket_server(cfg_ipc, ctx_ipc) {
                error!("UNIX Domain Socket IPC server terminated: {e:?}");
            }
        });

        // 3. Autonomous Maintenance Loop
        info!("Autonomous maintenance loop started. Interval: {} seconds.", self.config.interval_seconds);
        loop {
            thread::sleep(Duration::from_secs(self.config.interval_seconds));

            info!("Triggering autonomous scheduled ZRAM maintenance...");
            let summary = {
                let mut ctx = match self.context.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                ctx.do_maintenance(&self.config, false)
            };

            info!(
                "Maintenance cycle finished in {:?}. Status: {}. Before: {}B, After: {}B",
                summary.duration,
                summary.status_message,
                summary.before_compr_bytes,
                summary.after_compr_bytes
            );
        }
    }
}

fn run_socket_server(config: MmdConfig, context: Arc<Mutex<ZramContext>>) -> anyhow::Result<()> {
    let socket_path = &config.socket_path;

    // Check if an existing daemon is already actively listening on this socket
    if std::path::Path::new(socket_path).exists() {
        if let Ok(mut probe) = UnixStream::connect(socket_path) {
            let _ = probe.set_read_timeout(Some(Duration::from_millis(500)));
            let _ = probe.set_write_timeout(Some(Duration::from_millis(500)));
            if probe.write_all(b"ping\n").is_ok() {
                let mut line = String::new();
                let mut reader = BufReader::new(&probe);
                if reader.read_line(&mut line).is_ok() && line.trim() == "PONG" {
                    anyhow::bail!("Another active mmd daemon is already running on {socket_path}");
                }
            }
        }
        // Stale socket from previous crash or dead process, clean it up safely
        let _ = std::fs::remove_file(socket_path);
    }

    let listener = UnixListener::bind(socket_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o660));
    }
    info!("Lightweight IPC listening on UNIX socket: {}", socket_path);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let current_clients = ACTIVE_IPC_CLIENTS.load(Ordering::Relaxed);
                if current_clients >= MAX_CONCURRENT_IPC_CLIENTS {
                    warn!("Rejecting IPC connection: reached maximum concurrent clients ({MAX_CONCURRENT_IPC_CLIENTS})");
                    let mut s = stream;
                    let _ = writeln!(s, "ERROR: Server busy, maximum concurrent connections reached");
                    continue;
                }

                ACTIVE_IPC_CLIENTS.fetch_add(1, Ordering::Relaxed);
                let ctx = context.clone();
                let cfg = config.clone();
                thread::spawn(move || {
                    struct ConnGuard;
                    impl Drop for ConnGuard {
                        fn drop(&mut self) {
                            ACTIVE_IPC_CLIENTS.fetch_sub(1, Ordering::Relaxed);
                        }
                    }
                    let _guard = ConnGuard;

                    if let Err(e) = handle_client(stream, cfg, ctx) {
                        warn!("IPC client handler error: {e:?}");
                    }
                });
            }
            Err(e) => {
                error!("IPC accept error: {e:?}");
            }
        }
    }
    Ok(())
}

fn handle_client(
    mut stream: UnixStream,
    config: MmdConfig,
    context: Arc<Mutex<ZramContext>>,
) -> std::io::Result<()> {
    // Set safety timeouts to prevent hanging client connections
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    let clone = stream.try_clone()?;
    // Limit reading to at most 1024 bytes to prevent unbounded memory allocation
    let mut reader = BufReader::new(clone.take(1024));
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let command = line.trim();

    match command {
        "trigger" | "maintain" | "trigger_all" => {
            let force_all = command == "trigger_all";
            let summary = {
                let mut ctx = match context.lock() {
                    Ok(guard) => guard,
                    Err(p) => p.into_inner(),
                };
                ctx.do_maintenance(&config, force_all)
            };
            writeln!(
                stream,
                "OK: Maintenance executed in {:?}. Result: {}. Compression: {}B -> {}B",
                summary.duration,
                summary.status_message,
                summary.before_compr_bytes,
                summary.after_compr_bytes
            )?;
        }
        "status" => {
            // Read hardware device sysfs directly without holding global maintenance lock
            let zram0 = mmd::zram::SysfsZramApiImpl::new(0);
            let stat = ZramMmStat::load(&zram0).unwrap_or_default();
            let algo = zram0.read_comp_algorithm().unwrap_or_default();
            let recomp_algo = zram0.read_recomp_algorithm().unwrap_or_default();
            let disksize = zram0.read_disksize().unwrap_or_default();

            let last_maint = context.lock().map(|c| c.last_maintenance_at.elapsed()).unwrap_or_default();

            writeln!(stream, "ZRAM Device 0:")?;
            writeln!(stream, "  Disksize: {}", disksize.trim())?;
            writeln!(stream, "  Primary Algorithm: {}", algo.trim())?;
            writeln!(stream, "  Recomp Algorithm: {}", recomp_algo.trim())?;
            writeln!(stream, "  Orig Data Size: {} bytes ({} MiB)", stat.orig_data_size, stat.orig_data_size / (1024 * 1024))?;
            writeln!(stream, "  Compr Data Size: {} bytes ({} MiB)", stat.compr_data_size, stat.compr_data_size / (1024 * 1024))?;
            writeln!(stream, "  Mem Used Total: {} bytes ({} MiB)", stat.mem_used_total, stat.mem_used_total / (1024 * 1024))?;
            writeln!(stream, "  Last Maintenance: {:.1}s ago", last_maint.as_secs_f64())?;
        }
        "ping" => {
            writeln!(stream, "PONG")?;
        }
        _ => {
            writeln!(stream, "ERROR: Unknown command '{}'. Valid: trigger, status, ping", command)?;
        }
    }
    stream.flush()
}
