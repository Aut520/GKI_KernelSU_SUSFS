// Standalone mmd configuration with rich CLI and Environment parsing
use std::time::Duration;
use log::LevelFilter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MmdMode {
    Daemon,
    Trigger { force_all: bool },
    Status,
    Client(String),
    Help,
}

#[derive(Debug, Clone)]
pub struct MmdConfig {
    /// Maintenance loop interval in daemon mode (seconds)
    pub interval_seconds: u64,
    /// Minimum idle age for recompression (seconds)
    pub min_idle_seconds: u64,
    /// Maximum idle age for recompression (seconds)
    pub max_idle_seconds: u64,
    /// Minimum time between recompression runs (seconds)
    pub backoff_seconds: u64,
    /// Minimum incompressible threshold in bytes for recompression
    pub threshold_bytes: u64,
    /// Recompress huge and idle pages
    pub huge_idle: bool,
    /// Recompress idle pages
    pub idle: bool,
    /// Recompress huge pages
    pub huge: bool,
    /// Number of zram devices to manage
    pub num_devices: u64,
    /// UNIX domain socket path for lightweight IPC control
    pub socket_path: String,
    /// Logging verbosity level
    pub log_level: LevelFilter,
}

impl Default for MmdConfig {
    fn default() -> Self {
        Self {
            interval_seconds: 3600,       // 1 hour daemon loop
            min_idle_seconds: 2 * 3600,    // 2 hours minimum idle age
            max_idle_seconds: 4 * 3600,    // 4 hours maximum idle age
            backoff_seconds: 1800,        // 30 minutes backoff
            threshold_bytes: 1024,        // 1 KiB threshold
            huge_idle: true,
            idle: true,
            huge: true,
            num_devices: 1,
            socket_path: "/data/local/tmp/mmd.sock".to_string(),
            log_level: LevelFilter::Info,
        }
    }
}

/// Parse time string into seconds, supporting units: s (seconds), m (minutes), h (hours), d (days)
/// Example: "1800", "30m", "1h", "2h", "4h"
pub fn parse_duration_to_seconds(s: &str) -> Result<u64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Duration string cannot be empty".to_string());
    }
    if let Ok(num) = s.parse::<u64>() {
        return Ok(num);
    }
    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: u64 = num_str.parse().map_err(|_| format!("Invalid number in duration: '{s}'"))?;
    match unit.to_ascii_lowercase().as_str() {
        "s" => Ok(num),
        "m" => Ok(num * 60),
        "h" => Ok(num * 3600),
        "d" => Ok(num * 86400),
        _ => Err(format!("Unknown duration unit in '{s}'. Supported: s, m, h, d (or raw seconds)")),
    }
}

pub fn format_duration_human(secs: u64) -> String {
    if secs >= 86400 && secs % 86400 == 0 {
        format!("{}s ({}d)", secs, secs / 86400)
    } else if secs >= 3600 && secs % 3600 == 0 {
        format!("{}s ({}h)", secs, secs / 3600)
    } else if secs >= 60 && secs % 60 == 0 {
        format!("{}s ({}m)", secs, secs / 60)
    } else if secs >= 3600 {
        format!("{}s ({}h {}m)", secs, secs / 3600, (secs % 3600) / 60)
    } else if secs >= 60 {
        format!("{}s ({}m {}s)", secs, secs / 60, secs % 60)
    } else {
        format!("{}s", secs)
    }
}

impl MmdConfig {
    /// Loads default configuration, overlays environment variables, then parses CLI arguments.
    pub fn parse_args_and_env(args: &[String]) -> Result<(MmdMode, Self), String> {
        let mut cfg = Self::default();

        // 1. Overlay Environment Variables
        if let Ok(v) = std::env::var("MMD_INTERVAL") {
            if let Ok(num) = parse_duration_to_seconds(&v) { cfg.interval_seconds = num; }
        }
        if let Ok(v) = std::env::var("MMD_MIN_IDLE") {
            if let Ok(num) = parse_duration_to_seconds(&v) { cfg.min_idle_seconds = num; }
        }
        if let Ok(v) = std::env::var("MMD_MAX_IDLE") {
            if let Ok(num) = parse_duration_to_seconds(&v) { cfg.max_idle_seconds = num; }
        }
        if let Ok(v) = std::env::var("MMD_BACKOFF") {
            if let Ok(num) = parse_duration_to_seconds(&v) { cfg.backoff_seconds = num; }
        }
        if let Ok(v) = std::env::var("MMD_THRESHOLD") {
            if let Ok(num) = v.parse() { cfg.threshold_bytes = num; }
        }
        if let Ok(v) = std::env::var("MMD_SOCKET") {
            cfg.socket_path = v;
        }

        // 2. Parse CLI Arguments (Overrides environment variables)
        let mut mode = MmdMode::Daemon;
        let mut explicit_mode = false;
        let mut force_all_flag = false;

        let mut i = 1;
        while i < args.len() {
            let arg = &args[i];
            match arg.as_str() {
                "-d" | "--daemon" => {
                    mode = MmdMode::Daemon;
                    explicit_mode = true;
                }
                "-t" | "--trigger" => {
                    mode = MmdMode::Trigger { force_all: force_all_flag };
                    explicit_mode = true;
                }
                "-a" | "--all" => {
                    force_all_flag = true;
                    if let MmdMode::Trigger { ref mut force_all } = mode {
                        *force_all = true;
                    }
                }
                "-s" | "--status" => {
                    mode = MmdMode::Status;
                    explicit_mode = true;
                }
                "-h" | "--help" => {
                    return Ok((MmdMode::Help, cfg));
                }
                "-c" | "--client" => {
                    i += 1;
                    let cmd = if i < args.len() {
                        args[i].clone()
                    } else {
                        "trigger".to_string()
                    };
                    mode = MmdMode::Client(cmd);
                    explicit_mode = true;
                }
                "-i" | "--interval" => {
                    i += 1;
                    let val = args.get(i).ok_or_else(|| "Missing value for --interval".to_string())?;
                    cfg.interval_seconds = parse_duration_to_seconds(val)?;
                }
                "-m" | "--min-idle" => {
                    i += 1;
                    let val = args.get(i).ok_or_else(|| "Missing value for --min-idle".to_string())?;
                    cfg.min_idle_seconds = parse_duration_to_seconds(val)?;
                }
                "-M" | "--max-idle" => {
                    i += 1;
                    let val = args.get(i).ok_or_else(|| "Missing value for --max-idle".to_string())?;
                    cfg.max_idle_seconds = parse_duration_to_seconds(val)?;
                }
                "-b" | "--backoff" => {
                    i += 1;
                    let val = args.get(i).ok_or_else(|| "Missing value for --backoff".to_string())?;
                    cfg.backoff_seconds = parse_duration_to_seconds(val)?;
                }
                "-T" | "--threshold" => {
                    i += 1;
                    let val = args.get(i).ok_or_else(|| "Missing value for --threshold".to_string())?;
                    cfg.threshold_bytes = val.parse().map_err(|_| format!("Invalid threshold bytes: '{val}'"))?;
                }
                "-n" | "--devices" => {
                    i += 1;
                    let val = args.get(i).ok_or_else(|| "Missing value for --devices".to_string())?;
                    cfg.num_devices = val.parse().map_err(|_| format!("Invalid devices count: '{val}'"))?;
                }
                "-S" | "--socket" => {
                    i += 1;
                    cfg.socket_path = args.get(i).ok_or_else(|| "Missing value for --socket".to_string())?.clone();
                }
                "-v" | "--verbose" => {
                    cfg.log_level = LevelFilter::Debug;
                }
                "-q" | "--quiet" => {
                    cfg.log_level = LevelFilter::Warn;
                }
                "--log-level" => {
                    i += 1;
                    let val = args.get(i).ok_or_else(|| "Missing value for --log-level".to_string())?;
                    cfg.log_level = match val.to_lowercase().as_str() {
                        "trace" => LevelFilter::Trace,
                        "debug" => LevelFilter::Debug,
                        "info" => LevelFilter::Info,
                        "warn" => LevelFilter::Warn,
                        "error" => LevelFilter::Error,
                        _ => return Err(format!("Unknown log level: '{val}' (expected: trace, debug, info, warn, error)")),
                    };
                }
                // Key=Value variants
                arg if arg.starts_with("--interval=") => {
                    let val = &arg["--interval=".len()..];
                    cfg.interval_seconds = parse_duration_to_seconds(val)?;
                }
                arg if arg.starts_with("--min-idle=") => {
                    let val = &arg["--min-idle=".len()..];
                    cfg.min_idle_seconds = parse_duration_to_seconds(val)?;
                }
                arg if arg.starts_with("--max-idle=") => {
                    let val = &arg["--max-idle=".len()..];
                    cfg.max_idle_seconds = parse_duration_to_seconds(val)?;
                }
                arg if arg.starts_with("--backoff=") => {
                    let val = &arg["--backoff=".len()..];
                    cfg.backoff_seconds = parse_duration_to_seconds(val)?;
                }
                arg if arg.starts_with("--threshold=") => {
                    let val = &arg["--threshold=".len()..];
                    cfg.threshold_bytes = val.parse().map_err(|_| format!("Invalid threshold: '{val}'"))?;
                }
                arg if arg.starts_with("--socket=") => {
                    cfg.socket_path = arg["--socket=".len()..].to_string();
                }
                unknown => {
                    return Err(format!("Unrecognized argument: '{unknown}'. Run 'mmd --help' for usage."));
                }
            }
            i += 1;
        }

        if let MmdMode::Trigger { ref mut force_all } = mode {
            if force_all_flag {
                *force_all = true;
            }
        }

        if !explicit_mode && force_all_flag {
            mode = MmdMode::Trigger { force_all: true };
        }

        Ok((mode, cfg))
    }

    pub fn to_recompression_params(&self) -> mmd::zram::recompression::Params {
        mmd::zram::recompression::Params {
            backoff_duration: Duration::from_secs(self.backoff_seconds),
            min_idle: Duration::from_secs(self.min_idle_seconds),
            max_idle: Duration::from_secs(self.max_idle_seconds),
            huge_idle: self.huge_idle,
            idle: self.idle,
            huge: self.huge,
            threshold_bytes: self.threshold_bytes,
        }
    }
}
