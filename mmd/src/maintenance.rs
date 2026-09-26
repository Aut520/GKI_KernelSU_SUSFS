// Maintenance engine for standalone mmd
use std::time::Duration;
use std::time::Instant;

use log::{error, info, warn};
use mmd::os::MeminfoApiImpl;
use mmd::suspend_history::SuspendHistory;
use mmd::time::TimeApi;
use mmd::time::TimeApiImpl;
use mmd::zram::idle::IdleMarker;
use mmd::zram::idle::TrackedIdleMarker;
use mmd::zram::idle::UntrackedIdleMarker;
use mmd::zram::recompression::get_zram_recompression_status;
use mmd::zram::recompression::Error as ZramRecompressionError;
use mmd::zram::recompression::ZramRecompression;
use mmd::zram::recompression::ZramRecompressionStatus;
use mmd::zram::stats::ZramMmStat;
use mmd::zram::SysfsZramApi;
use mmd::zram::SysfsZramApiImpl;

use crate::config::MmdConfig;

pub enum ZramIdleMarker {
    Tracked(TrackedIdleMarker<SysfsZramApiImpl>),
    Untracked(UntrackedIdleMarker<TimeApiImpl>),
}

impl ZramIdleMarker {
    pub fn as_idle_marker(&self) -> &dyn IdleMarker {
        match self {
            ZramIdleMarker::Tracked(m) => m,
            ZramIdleMarker::Untracked(m) => m,
        }
    }
}

pub struct ZramContext {
    pub zram_devices: Vec<SysfsZramApiImpl>,
    pub zram_recompression: Option<ZramRecompression>,
    pub suspend_history: SuspendHistory,
    pub idle_marker: ZramIdleMarker,
    pub last_maintenance_at: Instant,
}

#[derive(Debug, Default)]
pub struct MaintenanceSummary {
    pub duration: Duration,
    pub before_orig_bytes: u64,
    pub before_compr_bytes: u64,
    pub after_orig_bytes: u64,
    pub after_compr_bytes: u64,
    pub recompress_success: bool,
    pub status_message: String,
}

impl ZramContext {
    pub fn new(num_devices: u64) -> Self {
        let zram_devices: Vec<_> = (0..num_devices).map(SysfsZramApiImpl::new).collect();

        let do_recompression: std::io::Result<_> = zram_devices.iter().try_fold(false, |acc, zram| {
            Ok(acc || (get_zram_recompression_status(zram)? == ZramRecompressionStatus::Activated))
        });

        let zram_recompression = match do_recompression {
            Ok(true) => {
                info!("ZRAM recompression is activated in kernel sysfs");
                Some(ZramRecompression::new())
            }
            Ok(false) => {
                warn!("ZRAM recompression is NOT configured in kernel sysfs (no secondary algorithm set)");
                None
            }
            Err(e) => {
                error!("Failed to check zram recompression status: {e:?}");
                None
            }
        };

        let mut idle_marker = ZramIdleMarker::Tracked(TrackedIdleMarker::new());
        if zram_recompression.is_some() {
            if let Some(first_zram) = zram_devices.first() {
                match is_idle_aging_supported(first_zram) {
                    Ok(true) => {
                        info!("Kernel supports precise idle aging (CONFIG_ZRAM_MEMORY_TRACKING)");
                    }
                    Ok(false) => {
                        warn!("Kernel lacks CONFIG_ZRAM_MEMORY_TRACKING. Falling back to UntrackedIdleMarker.");
                        let mut marker = UntrackedIdleMarker::<TimeApiImpl>::new();
                        for zram in zram_devices.iter() {
                            if let Err(e) = marker.refresh(zram) {
                                error!("Failed to refresh untracked idle marker on init: {e:?}");
                            }
                        }
                        idle_marker = ZramIdleMarker::Untracked(marker);
                    }
                    Err(e) => {
                        error!("Failed to check idle aging support: {e:?}");
                    }
                }
            }
        }

        Self {
            zram_devices,
            zram_recompression,
            suspend_history: SuspendHistory::new(),
            idle_marker,
            last_maintenance_at: Instant::now(),
        }
    }

    /// Performs one maintenance round (calculates dynamic cold threshold, marks idle, triggers recompression)
    pub fn do_maintenance(&mut self, config: &MmdConfig) -> MaintenanceSummary {
        let start_time = Instant::now();
        let mut summary = MaintenanceSummary::default();

        // 1. Capture before stats
        if let Some(zram0) = self.zram_devices.first() {
            if let Ok(stat) = ZramMmStat::load(zram0) {
                summary.before_orig_bytes = stat.orig_data_size;
                summary.before_compr_bytes = stat.compr_data_size;
            }
        }

        let recompression_params = config.to_recompression_params();
        let mut refresh_idle_pages = true;
        let mut maintenance_active = false;

        if let Some(recompression) = self.zram_recompression.as_mut() {
            for zram in self.zram_devices.iter() {
                let ok = Self::execute_recompression(
                    zram,
                    recompression,
                    &recompression_params,
                    &self.suspend_history,
                    self.idle_marker.as_idle_marker(),
                    &mut summary,
                );
                if !ok {
                    refresh_idle_pages = false;
                }
                maintenance_active = true;
            }
        } else {
            summary.status_message = "Recompression is not configured on zram devices".to_string();
        }

        // Untracked marker fallback refresh
        if refresh_idle_pages && maintenance_active {
            if let ZramIdleMarker::Untracked(marker) = &mut self.idle_marker {
                for zram in self.zram_devices.iter() {
                    if let Err(e) = marker.refresh(zram) {
                        error!("Failed to refresh untracked idle marker: {e:?}");
                    }
                }
            }
        }

        // 2. Capture after stats
        if let Some(zram0) = self.zram_devices.first() {
            if let Ok(stat) = ZramMmStat::load(zram0) {
                summary.after_orig_bytes = stat.orig_data_size;
                summary.after_compr_bytes = stat.compr_data_size;
            }
        }

        summary.duration = start_time.elapsed();
        self.last_maintenance_at = Instant::now();
        summary
    }

    fn execute_recompression(
        zram: &SysfsZramApiImpl,
        recompression: &mut ZramRecompression,
        params: &mmd::zram::recompression::Params,
        suspend_history: &SuspendHistory,
        idle_marker: &dyn IdleMarker,
        summary: &mut MaintenanceSummary,
    ) -> bool {
        let result = recompression.mark_and_recompress::<SysfsZramApiImpl, MeminfoApiImpl>(
            zram,
            params,
            suspend_history,
            idle_marker,
            TimeApiImpl::get_boot_time(),
        );

        match result {
            Ok(_) => {
                info!("ZRAM recompression successfully executed for zram{}", zram.idx());
                summary.recompress_success = true;
                summary.status_message = "Recompression succeeded".to_string();
                true
            }
            Err(ZramRecompressionError::BackoffTime) => {
                info!("ZRAM recompression skipped due to backoff time window");
                summary.status_message = "Skipped: within backoff window".to_string();
                false
            }
            Err(ZramRecompressionError::TryMarkIdleAgain) => {
                info!("ZRAM idle pages not ready yet; will try next maintenance cycle");
                summary.status_message = "Idle pages not ready".to_string();
                false
            }
            Err(e) => {
                error!("Failed to execute zram recompression: {e:?}");
                summary.status_message = format!("Error: {e:?}");
                true
            }
        }
    }
}

/// Checks whether the kernel supports precise idle aging by writing to /sys/block/zram0/idle
fn is_idle_aging_supported(zram: &SysfsZramApiImpl) -> std::io::Result<bool> {
    if let Err(e) = zram.set_idle(&u32::MAX.to_string()) {
        if e.kind() == std::io::ErrorKind::InvalidInput {
            Ok(false)
        } else {
            Err(e)
        }
    } else {
        Ok(true)
    }
}
