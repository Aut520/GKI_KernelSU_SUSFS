// Standalone and Lightweight Memory Management Daemon (mmd)
// For Android ZRAM Recompression & Multi-Comp

mod config;
mod daemon;
mod maintenance;

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};

use log::{info, LevelFilter};
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
  -t, --trigger           Trigger one-shot ZRAM maintenance and exit
  -s, --status            Print current ZRAM status and compression stats
  -c, --client <CMD>      Send command to running daemon via UNIX socket (trigger, status, ping)
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

fn query_status(ctx: &ZramContext) {
    println!("==================================================");
    println!("           ZRAM Multi-Comp Status Inspector       ");
    println!("==================================================");

    for zram in &ctx.zram_devices {
        println!("Device: /dev/block/zram{}", zram.idx());
        let disksize = zram.read_disksize().unwrap_or_else(|_| "N/A".to_string());
        println!("  Disksize: {}", disksize.trim());

        let comp_algo = zram.read_comp_algorithm().unwrap_or_else(|_| "N/A".to_string());
        println!("  Primary Algorithm: {}", comp_algo.trim());

        let recomp_algo = zram.read_recomp_algorithm().unwrap_or_else(|_| "N/A".to_string());
        println!("  Secondary (Recomp) Algorithm: {}", recomp_algo.trim());

        if let Ok(stat) = ZramMmStat::load(zram) {
            let orig_mb = stat.orig_data_size as f64 / (1024.0 * 1024.0);
            let compr_mb = stat.compr_data_size as f64 / (1024.0 * 1024.0);
            let total_mb = stat.mem_used_total as f64 / (1024.0 * 1024.0);
            let ratio = if stat.compr_data_size > 0 {
                stat.orig_data_size as f64 / stat.compr_data_size as f64
            } else {
                0.0
            };

            println!("  Original Data Size : {:>10.2} MiB ({} bytes)", orig_mb, stat.orig_data_size);
            println!("  Compressed Data Size: {:>10.2} MiB ({} bytes)", compr_mb, stat.compr_data_size);
            println!("  Memory Used Total   : {:>10.2} MiB ({} bytes)", total_mb, stat.mem_used_total);
            println!("  Compression Ratio   : {:>10.2} : 1", ratio);
            println!("  Same Element Pages  : {}", stat.same_pages);
            if let Some(huge) = stat.huge_pages {
                println!("  Huge Pages          : {}", huge);
            }
        }
    }
    println!("==================================================");
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
            info!("One-shot ZRAM maintenance trigger initiated...");
            let mut context = ZramContext::new(config.num_devices);
            let summary = context.do_maintenance(&config);
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
                println!("  RAM Saved: {:.2} KiB ({} bytes)", saved as f64 / 1024.0, saved);
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
