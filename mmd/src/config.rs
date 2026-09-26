// Standalone mmd configuration
use std::time::Duration;

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
        }
    }
}

impl MmdConfig {
    pub fn load_from_env() -> Self {
        let mut cfg = Self::default();

        if let Ok(v) = std::env::var("MMD_INTERVAL") {
            if let Ok(num) = v.parse() { cfg.interval_seconds = num; }
        }
        if let Ok(v) = std::env::var("MMD_MIN_IDLE") {
            if let Ok(num) = v.parse() { cfg.min_idle_seconds = num; }
        }
        if let Ok(v) = std::env::var("MMD_MAX_IDLE") {
            if let Ok(num) = v.parse() { cfg.max_idle_seconds = num; }
        }
        if let Ok(v) = std::env::var("MMD_BACKOFF") {
            if let Ok(num) = v.parse() { cfg.backoff_seconds = num; }
        }
        if let Ok(v) = std::env::var("MMD_THRESHOLD") {
            if let Ok(num) = v.parse() { cfg.threshold_bytes = num; }
        }
        if let Ok(v) = std::env::var("MMD_SOCKET") {
            cfg.socket_path = v;
        }

        cfg
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
