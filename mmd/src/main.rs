// Standalone and Lightweight Memory Management Daemon (mmd)
// For Android ZRAM Recompression & Multi-Comp

mod config;
mod daemon;
mod maintenance;

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};

use log::{info, LevelFilter};
use mmd::zram::recompression::{get_zram_recompression_status, ZramRecompressionStatus};
use mmd::zram::stats::ZramMmStat;
use mmd::zram::SysfsZramApi;

use crate::config::MmdConfig;
use crate::daemon::MmdDaemon;
use crate::maintenance::ZramContext;

fn print_help() {
    println!(r#"
MMD (Memory Management Daemon) - Standalone ZRAM Multi-Comp Edition
Usage:
  mmd [OPTIONS]

Options:
  -d, --daemon            Run in autonomous daemon mode (default)
  -t, --trigger [-a]      Trigger one-shot ZRAM maintenance and exit
                          Add -a, --all to force mark all pages as idle for immediate recompression test
  -s, --status            Print comprehensive ZRAM status, multi-comp health and diagnostics
  -c, --client <CMD>      Send command to running daemon via UNIX socket (trigger, trigger_all, status, ping)
  -h, --help              Show this help message

Environment Variables:
  MMD_INTERVAL            Daemon check interval in seconds (default: 3600)
  MMD_MIN_IDLE            Minimum idle age in seconds (default: 7200 = 2h)
  MMD_MAX_IDLE            Maximum idle age in seconds (default: 14400 = 4h)
  MMD_BACKOFF             Minimum duration between recompressions (default: 1800 = 30m)
  MMD_THRESHOLD           Incompressible threshold in bytes (default: 1024)
  MMD_SOCKET              UNIX domain socket path (default: /data/local/tmp/mmd.sock)
"#);
}

#[derive(Default)]
struct SysMemInfo {
    mem_total_kb: u64,
    mem_free_kb: u64,
    mem_available_kb: u64,
    swap_total_kb: u64,
    swap_free_kb: u64,
}

fn read_system_meminfo() -> Option<SysMemInfo> {
    let content = std::fs::read_to_string("/proc/meminfo").ok()?;
    let mut info = SysMemInfo::default();
    for line in content.lines() {
        let mut parts = line.split_whitespace();
        let key = parts.next().unwrap_or("");
        let val: u64 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        match key {
            "MemTotal:" => info.mem_total_kb = val,
            "MemFree:" => info.mem_free_kb = val,
            "MemAvailable:" => info.mem_available_kb = val,
            "SwapTotal:" => info.swap_total_kb = val,
            "SwapFree:" => info.swap_free_kb = val,
            _ => {}
        }
    }
    Some(info)
}

fn read_uptime_secs() -> Option<u64> {
    let content = std::fs::read_to_string("/proc/uptime").ok()?;
    let first = content.split_whitespace().next()?;
    let secs: f64 = first.parse().ok()?;
    Some(secs as u64)
}

struct BlockStateCounts {
    idle_pages: u64,
    recompressed_pages: u64,
}

fn count_debug_block_state(zram_idx: u64) -> Option<BlockStateCounts> {
    let path = format!("/sys/kernel/debug/zram/zram{}/block_state", zram_idx);
    let file = std::fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut idle = 0u64;
    let mut recomp = 0u64;

    for line in reader.lines().map_while(Result::ok) {
        if let Some(flags) = line.split_whitespace().nth(2) {
            if flags.contains('i') {
                idle += 1;
            }
            if flags.contains('r') {
                recomp += 1;
            }
        }
    }
    Some(BlockStateCounts {
        idle_pages: idle,
        recompressed_pages: recomp,
    })
}

fn query_status(ctx: &ZramContext) {
    println!("======================================================================");
    println!("               ZRAM Multi-Comp Status & Health Inspector              ");
    println!("======================================================================");

    let uptime_opt = read_uptime_secs();
    let meminfo_opt = read_system_meminfo();

    for zram in &ctx.zram_devices {
        let idx = zram.idx();
        println!("[Device & Topology: /dev/block/zram{}]", idx);

        let disksize_raw = zram.read_disksize().unwrap_or_else(|_| "0".to_string());
        let disksize_bytes: u64 = disksize_raw.trim().parse().unwrap_or(0);
        let disksize_gib = disksize_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        println!("  Disk Capacity       : {:>6.2} GiB ({} bytes)", disksize_gib, disksize_bytes);

        let backing_dev = zram.read_backing_dev().unwrap_or_else(|_| "none".to_string());
        let backing_str = backing_dev.trim();
        println!("  Backing Device      : {}", if backing_str.is_empty() || backing_str == "none" { "None (Pure RAM swap)" } else { backing_str });

        let streams = zram.read_max_comp_streams().unwrap_or_else(|_| "N/A".to_string());
        println!("  Max Comp Streams    : {}", streams.trim());
        println!();

        println!("[Compression Engines & State]");
        let comp_algo = zram.read_comp_algorithm().unwrap_or_else(|_| "N/A".to_string());
        println!("  Primary Algorithm   : {}", comp_algo.trim());

        let recomp_algo = zram.read_recomp_algorithm().unwrap_or_else(|_| "N/A".to_string());
        let recomp_str = recomp_algo.trim();
        println!("  Secondary (Recomp)  : {}", if recomp_str.is_empty() { "None" } else { recomp_str });

        let status = get_zram_recompression_status(zram).unwrap_or(ZramRecompressionStatus::Unsupported);
        let status_desc = match status {
            ZramRecompressionStatus::Activated => "[ACTIVATED] Secondary recompression backend is active and ready",
            ZramRecompressionStatus::NotConfigured => "[NOT CONFIGURED] Kernel multi-comp supported but secondary algorithm not set",
            ZramRecompressionStatus::Unsupported => "[UNSUPPORTED] Kernel lacks CONFIG_ZRAM_MULTI_COMP sysfs node",
        };
        println!("  Multi-Comp Feature  : {}", status_desc);
        println!();

        if let Ok(stat) = ZramMmStat::load(zram) {
            println!("[Memory & Compression Metrics]");
            let orig_mb = stat.orig_data_size as f64 / (1024.0 * 1024.0);
            let compr_mb = stat.compr_data_size as f64 / (1024.0 * 1024.0);
            let total_mb = stat.mem_used_total as f64 / (1024.0 * 1024.0);
            let raw_ratio = if stat.compr_data_size > 0 {
                stat.orig_data_size as f64 / stat.compr_data_size as f64
            } else {
                0.0
            };
            let eff_ratio = if stat.mem_used_total > 0 {
                stat.orig_data_size as f64 / stat.mem_used_total as f64
            } else {
                0.0
            };
            let ram_saved_bytes = stat.orig_data_size.saturating_sub(stat.mem_used_total);
            let ram_saved_mb = ram_saved_bytes as f64 / (1024.0 * 1024.0);
            let pool_overhead_pct = if stat.mem_used_total > stat.compr_data_size && stat.mem_used_total > 0 {
                ((stat.mem_used_total - stat.compr_data_size) as f64 / stat.mem_used_total as f64) * 100.0
            } else {
                0.0
            };

            println!("  Original Data Size  : {:>8.2} MiB ({} bytes)", orig_mb, stat.orig_data_size);
            println!("  Compressed Physical : {:>8.2} MiB ({} bytes)", compr_mb, stat.compr_data_size);
            println!("  Memory Pool Used    : {:>8.2} MiB ({} bytes)", total_mb, stat.mem_used_total);
            println!("  Physical RAM Saved  : {:>8.2} MiB (orig - pool_used)", ram_saved_mb);
            println!("  Raw Compression     : {:>8.2} : 1 (orig / compr)", raw_ratio);
            println!("  Effective Ratio     : {:>8.2} : 1 (orig / pool_used)", eff_ratio);
            println!("  Pool Overhead       : {:>8.2} %   (allocator fragmentation & metadata)", pool_overhead_pct);

            let max_used_mb = (stat.mem_used_max as f64) / (1024.0 * 1024.0);
            println!("  Max Memory Consumed : {:>8.2} MiB", max_used_mb);
            if stat.mem_limit > 0 {
                println!("  Memory Hard Limit   : {:>8.2} MiB", stat.mem_limit as f64 / (1024.0 * 1024.0));
            } else {
                println!("  Memory Hard Limit   :     None");
            }
            println!();

            println!("[Page Classification & Distribution]");
            let total_stored_pages = stat.orig_data_size / 4096;
            let same_mb = (stat.same_pages * 4096) as f64 / (1024.0 * 1024.0);
            println!("  Total Stored Pages  : {} pages (~{} MiB uncompressed)", total_stored_pages, orig_mb as u64);
            println!("  Same Element (Zero) : {} pages ({:.2} MiB saved with 0 byte pool alloc)", stat.same_pages, same_mb);
            if let Some(huge) = stat.huge_pages {
                let huge_mb = (huge * 4096) as f64 / (1024.0 * 1024.0);
                println!("  Huge (Incompressible: {} pages ({:.2} MiB stored uncompressed)", huge, huge_mb);
            }
            println!("  Pages Compacted     : {} pages freed by zsmalloc compaction", stat.pages_compacted);
            println!();
        }

        println!("[System Memory & Swap Context]");
        if let Some(uptime) = uptime_opt {
            let h = uptime / 3600;
            let m = (uptime % 3600) / 60;
            let s = uptime % 60;
            println!("  System Uptime       : {}h {}m {}s", h, m, s);
        }
        if let Some(mem) = &meminfo_opt {
            let total_gib = mem.mem_total_kb as f64 / (1024.0 * 1024.0);
            let avail_gib = mem.mem_available_kb as f64 / (1024.0 * 1024.0);
            let free_gib = mem.mem_free_kb as f64 / (1024.0 * 1024.0);
            println!("  System RAM Total    : {:>6.2} GiB (Avail: {:.2} GiB, Free: {:.2} GiB)", total_gib, avail_gib, free_gib);

            let swap_tot_gib = mem.swap_total_kb as f64 / (1024.0 * 1024.0);
            let swap_free_gib = mem.swap_free_kb as f64 / (1024.0 * 1024.0);
            let swap_used_kb = mem.swap_total_kb.saturating_sub(mem.swap_free_kb);
            let swap_used_gib = swap_used_kb as f64 / (1024.0 * 1024.0);
            let swap_used_pct = if mem.swap_total_kb > 0 {
                (swap_used_kb as f64 / mem.swap_total_kb as f64) * 100.0
            } else {
                0.0
            };
            println!("  Swap Total / Used   : {:>6.2} GiB / {:.2} GiB ({:.1}% used, Free: {:.2} GiB)", swap_tot_gib, swap_used_gib, swap_used_pct, swap_free_gib);
        }
        println!();

        println!("[Kernel Capabilities & Diagnostics]");
        let precise_idle = std::fs::read_to_string("/sys/kernel/debug/zram/zram0/block_state").is_ok()
            || std::path::Path::new("/sys/block/zram0/idle").exists();
        println!("  Idle Aging Support  : {}", if precise_idle { "[YES] /sys/block/zram0/idle available" } else { "[NO]" });

        let recompress_writable = std::fs::OpenOptions::new().write(true).open("/sys/block/zram0/recompress").is_ok();
        println!("  Recompress Sysfs    : {}", if recompress_writable { "[YES] Writable" } else { "[NOTICE] Read-only or requires root" });

        if let Some(counts) = count_debug_block_state(idx) {
            println!("  Block State Sample  : Currently Idle (i): {} pages | Recompressed (r): {} pages", counts.idle_pages, counts.recompressed_pages);
        } else {
            println!("  Block State Sample  : N/A (debugfs restricted by SELinux or not mounted)");
        }

        println!("  Diagnostics Summary :");
        if let Some(uptime) = uptime_opt {
            if uptime < 7200 {
                println!("    * [Notice] Device uptime is under 2 hours (fresh boot).");
                println!("      The automatic daemon requires 2h idle aging before routine recompression.");
                println!("      To trigger an immediate test recompression on ALL current pages, run:");
                println!("        mmd -t --all");
            } else {
                println!("    * Device uptime exceeds 2 hours. Autonomous maintenance will run on cold pages.");
            }
        }
    }
    println!("======================================================================");
}

fn run_client(socket_path: &str, command: &str) -> anyhow::Result<()> {
    let mut stream = UnixStream::connect(socket_path)?;
    writeln!(stream, "{command}")?;
    stream.flush()?;

    let reader = BufReader::new(stream);
    for line in reader.lines() {
        println!("{}", line?);
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    // Initialize logging
    env_logger::Builder::from_default_env()
        .filter_level(LevelFilter::Info)
        .init();

    let args: Vec<String> = std::env::args().collect();
    let config = MmdConfig::load_from_env();

    let first_arg = args.get(1).map(|s| s.as_str()).unwrap_or("-d");

    match first_arg {
        "-h" | "--help" => {
            print_help();
            return Ok(());
        }
        "-s" | "--status" => {
            let context = ZramContext::new(config.num_devices);
            query_status(&context);
            return Ok(());
        }
        "-t" | "--trigger" => {
            let force_all = args.iter().any(|a| a == "-a" || a == "--all");
            let uptime_secs = read_uptime_secs().unwrap_or(0);
            let auto_all = !force_all && (uptime_secs > 0 && uptime_secs < config.min_idle_seconds);

            let effective_all = force_all || auto_all;
            if force_all {
                info!("One-shot ZRAM maintenance trigger initiated with --all (force marking all pages as IDLE)...");
            } else if auto_all {
                info!(
                    "System uptime ({}s) is less than min_idle ({}s). Automatically marking all pages as IDLE for test trigger...",
                    uptime_secs, config.min_idle_seconds
                );
            } else {
                info!("One-shot ZRAM maintenance trigger initiated...");
            }

            let mut context = ZramContext::new(config.num_devices);
            let summary = context.do_maintenance(&config, effective_all);
            println!("--------------------------------------------------");
            println!("ZRAM Maintenance Report:");
            println!("  Status: {}", summary.status_message);
            println!("  Elapsed Time: {:?}", summary.duration);
            println!(
                "  Compressed Size: {:.2} MiB -> {:.2} MiB",
                summary.before_compr_bytes as f64 / (1024.0 * 1024.0),
                summary.after_compr_bytes as f64 / (1024.0 * 1024.0)
            );
            if summary.before_compr_bytes > summary.after_compr_bytes {
                let saved = summary.before_compr_bytes - summary.after_compr_bytes;
                let pct = (saved as f64 / summary.before_compr_bytes as f64) * 100.0;
                println!(
                    "  RAM Saved: {:.2} MiB ({} bytes, -{:.1}%)",
                    saved as f64 / (1024.0 * 1024.0),
                    saved,
                    pct
                );
            } else {
                println!("  RAM Saved: 0 bytes (No eligible idle/incompressible pages to recompress)");
            }
            println!("--------------------------------------------------");
            return Ok(());
        }
        "-c" | "--client" => {
            let cmd = args.get(2).map(|s| s.as_str()).unwrap_or("trigger");
            if let Err(e) = run_client(&config.socket_path, cmd) {
                eprintln!("Failed to connect to mmd daemon at {}: {:?}", config.socket_path, e);
                std::process::exit(1);
            }
            return Ok(());
        }
        "-d" | "--daemon" | _ => {
            let context = Arc::new(Mutex::new(ZramContext::new(config.num_devices)));
            let daemon = MmdDaemon::new(config, context);
            daemon.run()?;
        }
    }

    Ok(())
}
