// Daemon runtime with autonomous loop and lightweight UNIX Domain Socket IPC
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
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

        // 1. Spawn Suspend Monitor Thread (1-hour resolution for sleep tracking)
        let ctx_clone = self.context.clone();
        let max_idle_duration = Duration::from_secs(self.config.max_idle_seconds);
        thread::spawn(move || {
            let mut suspend_monitor = SuspendMonitor::<TimeApiImpl>::new();
            loop {
                thread::sleep(Duration::from_secs(3600));
                let (suspend_duration, now_boot) = suspend_monitor.generate_suspend_duration();
                if let Ok(mut ctx) = ctx_clone.lock() {
                    ctx.suspend_history.record_suspend_duration(
                        suspend_duration,
                        now_boot,
                        max_idle_duration,
                    );
                    info!(
                        "Recorded suspend duration: {:?}, boot_time: {:?}",
                        suspend_duration, now_boot
                    );
                }
            }
        });

        // 2. Spawn UNIX Domain Socket IPC Server (Non-blocking, minimal overhead)
        let ctx_ipc = self.context.clone();
        let cfg_ipc = self.config.clone();
        thread::spawn(move || {
            if let Err(e) = run_socket_server(cfg_ipc, ctx_ipc) {
                warn!("UNIX Domain Socket IPC server exited: {e:?}");
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
                ctx.do_maintenance(&self.config)
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
    let _ = std::fs::remove_file(socket_path);

    let listener = UnixListener::bind(socket_path)?;
    info!("Lightweight IPC listening on UNIX socket: {}", socket_path);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let ctx = context.clone();
                let cfg = config.clone();
                thread::spawn(move || {
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
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let command = line.trim();

    match command {
        "trigger" | "maintain" => {
            let summary = {
                let mut ctx = match context.lock() {
                    Ok(guard) => guard,
                    Err(p) => p.into_inner(),
                };
                ctx.do_maintenance(&config)
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
            let ctx = match context.lock() {
                Ok(guard) => guard,
                Err(p) => p.into_inner(),
            };
            if let Some(zram0) = ctx.zram_devices.first() {
                let stat = ZramMmStat::load(zram0).unwrap_or_default();
                let algo = zram0.read_comp_algorithm().unwrap_or_default();
                let recomp_algo = zram0.read_recomp_algorithm().unwrap_or_default();
                let disksize = zram0.read_disksize().unwrap_or_default();

                writeln!(stream, "ZRAM Device 0:")?;
                writeln!(stream, "  Disksize: {}", disksize.trim())?;
                writeln!(stream, "  Primary Algorithm: {}", algo.trim())?;
                writeln!(stream, "  Recomp Algorithm: {}", recomp_algo.trim())?;
                writeln!(stream, "  Orig Data Size: {} bytes ({} MiB)", stat.orig_data_size, stat.orig_data_size / (1024 * 1024))?;
                writeln!(stream, "  Compr Data Size: {} bytes ({} MiB)", stat.compr_data_size, stat.compr_data_size / (1024 * 1024))?;
                writeln!(stream, "  Mem Used Total: {} bytes ({} MiB)", stat.mem_used_total, stat.mem_used_total / (1024 * 1024))?;
            } else {
                writeln!(stream, "No ZRAM devices found")?;
            }
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
