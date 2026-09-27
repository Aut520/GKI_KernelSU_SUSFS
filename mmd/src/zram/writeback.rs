// Copyright 2024, The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! This module provides policy to manage zram writeback feature.
//!
//! See "writeback" section in the kernel document for details.
//!
//! https://www.kernel.org/doc/Documentation/blockdev/zram.txt

mod history;
// #[cfg(test)]
// mod tests;

use std::os::fd::OwnedFd;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::os::MeminfoApi;
use crate::suspend_history::SuspendHistory;
use crate::time::BootTime;
use crate::zram::idle::calculate_idle_time;
use crate::zram::idle::IdleMarker;
use crate::zram::per_process_ioctls::PerProcessIoctls;
use crate::zram::stats::ZramBdStat;
use crate::zram::writeback::history::ZramWritebackHistory;
use crate::zram::SysfsZramApi;

/// The size of a zram writeback page in bytes.
///
/// The zram kernel module treats page size for writeback as 4KB even if the
/// physical page size is different (e.g. `/sys/block/zram0/bd_stat`,
/// `/sys/block/zram0/writeback_limit`).
///
/// [ZramWriteback] uses 4KB as the page size for all calculations.
pub const WRITEBACK_PAGE_SIZE: u64 = 4096;

// The kernel serializes all post processing operations. Since prefetch occurs
// during app launch and is thus latency sensitive, it needs to be able to
// interrupt other post-processing operations. To accomplish this, we limit any
// individual writeback/recompression operation to 1MiB, so we can check abort
// flags without too much latency.
const PER_PROCESS_WRITEBACK_MAX_OP_SIZE: u64 = 1024 * 1024;

/// Error from [ZramWriteback].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// writeback too frequently
    #[error("writeback too frequently")]
    BackoffTime,
    /// no more space for zram writeback
    #[error("no pages in zram for zram writeback")]
    Limit,
    /// failed to parse writeback_limit
    #[error("failed to parse writeback_limit")]
    InvalidWritebackLimit,
    /// failure on setting zram idle
    #[error("calculate zram idle {0}")]
    CalculateIdle(#[from] crate::zram::idle::CalculateError),
    /// failure on setting zram idle
    #[error("set zram idle {0}")]
    MarkIdle(Box<dyn std::error::Error>),
    /// you need to mark idle later again.
    #[error("idle pages are not ready to mark yet")]
    TryMarkIdleAgain,
    /// failure on writing to /sys/block/zram0/writeback
    #[error("writeback: {0}")]
    Writeback(std::io::Error),
    /// failure on access to /sys/block/zram0/writeback_limit
    #[error("writeback_limit: {0}")]
    WritebackLimit(std::io::Error),
    /// failure of process writeback due to exceeding the daily limit
    #[error("writeback exceeded daily limit")]
    WritebackDailyLimitExceeded,
    /// failure of process writeback due to backing device being full
    #[error("writeback failed due full backing device")]
    WritebackNoSpace,
    /// failure loading zram stats from sysfs
    #[error("failed loading zram stats: {0}")]
    LoadStatsError(crate::zram::stats::Error),
}

type Result<T> = std::result::Result<T, Error>;

/// Current zram writeback setup status
#[derive(Debug, PartialEq)]
pub enum ZramWritebackStatus {
    /// Zram writeback is not supported by the kernel.
    Unsupported,
    /// Zram writeback is supported but not configured yet.
    NotConfigured,
    /// Zram writeback was already activated.
    Activated,
}

/// Whether the zram writeback is activated on the device or not.
pub fn get_zram_writeback_status<Z: SysfsZramApi>(
    zram: &Z,
) -> std::io::Result<ZramWritebackStatus> {
    match zram.read_backing_dev() {
        // If /sys/block/zram0/backing_dev is "none", zram writeback is not configured yet.
        Ok(backing_dev) => {
            if backing_dev.trim() == "none" {
                Ok(ZramWritebackStatus::NotConfigured)
            } else {
                Ok(ZramWritebackStatus::Activated)
            }
        }
        // If it can't access /sys/block/zram0/backing_dev, zram writeback feature is disabled on
        // the kernel.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ZramWritebackStatus::Unsupported),
        Err(e) => Err(e),
    }
}

/// The parameters for zram writeback that can vary per zram device.
pub struct PerDeviceParams {
    /// Whether writeback huge and idle pages or not.
    pub huge_idle: bool,
    /// Whether writeback idle pages or not.
    pub idle: bool,
    /// Whether writeback huge pages or not.
    pub huge: bool,
}

impl Default for PerDeviceParams {
    fn default() -> Self {
        Self { huge_idle: true, idle: true, huge: false }
    }
}

/// The parameters for zram writeback.
pub struct Params {
    /// Whether zram writeback limit is enabled or not.
    pub limit_enabled: bool,
    /// The backoff time since last writeback.
    pub backoff_duration: Duration,
    /// The minimum idle duration to writeback. This is used for [calculate_idle_time].
    pub min_idle: Duration,
    /// The maximum idle duration to writeback. This is used for [calculate_idle_time].
    pub max_idle: Duration,
    /// Minimum bytes to writeback in 1 round.
    pub min_bytes: u64,
    /// Maximum bytes to writeback in 1 round.
    pub max_bytes: u64,
    /// Maximum bytes to writeback allowed in a day.
    pub max_bytes_per_day: u64,
    /// Parameters that can vary per zram device.
    pub per_device_params: Vec<PerDeviceParams>,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            limit_enabled: true,
            // 10 minutes
            backoff_duration: Duration::from_secs(600),
            // 20 hours
            min_idle: Duration::from_secs(20 * 3600),
            // 25 hours
            max_idle: Duration::from_secs(25 * 3600),
            per_device_params: vec![PerDeviceParams::default()],
            // 5 MiB
            min_bytes: 5 << 20,
            // 300 MiB
            max_bytes: 300 << 20,
            // 1 GiB
            max_bytes_per_day: 1 << 30,
        }
    }
}

/// The stats for zram writeback.
#[derive(Debug, Default)]
pub struct Stats {
    /// orig_data_size of [crate::zram::stats::ZramMmStat].
    pub orig_data_size: u64,
    /// bd_count_pages of [crate::zram::stats::ZramBdStat]. Unit: 4KB
    pub current_writeback_pages_4k: u64,
}

/// The detailed of the writeback limit applied to a zram writeback attempt.
#[derive(Debug)]
pub struct WritebackLimitDetails {
    /// Calculated writeback limit pages.
    pub pages_4k: u64,
    /// Writeback daily limit pages. This is calculated from the total written
    /// back page per day.
    pub daily_pages_4k: u64,
    /// The content of /sys/block/zram0/writeback_limit just before starting
    /// zram writeback. This is usually equals to the smaller of limit_pages and
    /// daily_limit_pages unless kernel tweaks the updated writeback_limit
    /// value.
    pub actual_pages_4k: u64,
}

/// The detailed results of a zram writeback attempt.
#[derive(Debug, Default)]
pub struct WritebackDetails {
    /// WritebackModeDetails for huge idle pages.
    pub huge_idle: WritebackModeDetails,
    /// WritebackModeDetails for idle pages.
    pub idle: WritebackModeDetails,
    /// WritebackModeDetails for huge pages.
    pub huge: WritebackModeDetails,
    /// Information about the attempt's writeback limit.
    pub limit: Option<WritebackLimitDetails>,
}

/// The detailed results of a zram writeback attempt per zram page type (i.e.
/// huge_idle, idle, huge pages).
#[derive(Debug, Default)]
pub struct WritebackModeDetails {
    /// Number of pages written back.
    pub written_pages_4k: u64,
}

enum Mode {
    HugeIdle,
    Idle,
    Huge,
}

fn load_current_writeback_limit<Z: SysfsZramApi>(zram: &Z) -> Result<u64> {
    let contents = zram.read_writeback_limit().map_err(Error::WritebackLimit)?;
    contents.trim().parse().map_err(|_| Error::InvalidWritebackLimit)
}

fn load_effective_writeback_limit<Z: SysfsZramApi>(zram: &Z, params: &Params) -> Option<u64> {
    if params.limit_enabled {
        // If reading writeback_limit fails, we assume that all
        // writeback_limit was consumed conservatively.
        Some(load_current_writeback_limit(zram).unwrap_or(0))
    } else {
        None
    }
}

/// ZramWriteback manages zram writeback policies.
pub struct ZramWriteback {
    history: ZramWritebackHistory,
    last_writeback_at: Option<BootTime>,
    total_zram_pages_4k: u64,
    zram_writeback_pages_4k: u64,
}

fn map_writeback_error(remaining_writeback_limit: &Option<u64>, e: std::io::Error) -> Error {
    match (e.raw_os_error(), remaining_writeback_limit) {
        // If wbd_wb_limitriteback reaches writeback_limit, the kernel will return
        // EIO. The kernel shouldn't do any IO between decrementing the limit
        // and re-checking the limit, so this shouldn't result in false positives.
        (Some(libc::EIO), Some(0)) => Error::WritebackDailyLimitExceeded,
        // If the backing device fills up, the kernel returns ENOSPC.
        (Some(libc::ENOSPC), _) => Error::WritebackNoSpace,
        _ => Error::Writeback(e),
    }
}

#[derive(Default)]
/// State to pass to Writeback::writeback_process_zram_memory() to allow it to
/// resume after being interrupted.
pub struct PerProcessWbState {
    cur_device_idx: usize,
    start_addr: u64,
}

impl ZramWriteback {
    /// Creates a new [ZramWriteback].
    pub fn new(total_zram_size: u64, zram_writeback_size: u64) -> Self {
        let total_zram_pages_4k = total_zram_size / WRITEBACK_PAGE_SIZE;
        let zram_writeback_pages_4k = zram_writeback_size / WRITEBACK_PAGE_SIZE;
        assert!(total_zram_pages_4k != 0);

        Self {
            history: ZramWritebackHistory::new(),
            last_writeback_at: None,
            total_zram_pages_4k,
            zram_writeback_pages_4k,
        }
    }

    fn update_writeback_limit<Z: SysfsZramApi>(
        &mut self,
        zram: &Z,
        params: &Params,
        operation_writeback_limit: Option<u64>,
        now: BootTime,
        metric: Option<&mut WritebackDetails>,
    ) -> Result<Option<u64>> {
        if !params.limit_enabled {
            return Ok(None);
        }

        self.history.cleanup(now);
        let daily_limit_pages_4k =
            self.history.calculate_daily_limit(params.max_bytes_per_day / WRITEBACK_PAGE_SIZE, now);
        let applied_limit_pages_4k = if let Some(limit_pages_4k) = operation_writeback_limit {
            std::cmp::min(limit_pages_4k, daily_limit_pages_4k)
        } else {
            daily_limit_pages_4k
        };

        if applied_limit_pages_4k == 0 {
            return Err(Error::Limit);
        }
        zram.write_writeback_limit(&applied_limit_pages_4k.to_string())
            .map_err(Error::WritebackLimit)?;
        let writeback_limit = load_current_writeback_limit(zram)?;

        if let Some(metric) = metric {
            metric.limit = Some(WritebackLimitDetails {
                pages_4k: operation_writeback_limit.unwrap_or(u64::MAX),
                daily_pages_4k: daily_limit_pages_4k,
                actual_pages_4k: writeback_limit,
            });
        }
        Ok(Some(writeback_limit))
    }

    /// Writes back idle or huge zram pages to disk.
    ///
    /// For zram_device[i], the PerDeviceParams para.per_device_params[i] is used. If
    /// i is out of bounds, writeback for zram_device[i] is skipped.
    pub fn mark_and_flush_pages<Z: SysfsZramApi, M: MeminfoApi>(
        &mut self,
        zram_devices: &[&Z],
        params: &Params,
        stats: &Stats,
        suspend_history: &SuspendHistory,
        idle_marker: &dyn IdleMarker,
        now: BootTime,
    ) -> Result<WritebackDetails> {
        if let Some(last_at) = self.last_writeback_at {
            if now.saturating_duration_since(last_at) < params.backoff_duration {
                return Err(Error::BackoffTime);
            }
        }

        let mut details = WritebackDetails::default();
        let mut limit_pages_4k = self.calculate_idle_writeback_limit(params, stats);

        for (idx, zram) in zram_devices.iter().enumerate() {
            let Some(per_device_params) = params.per_device_params.get(idx) else {
                continue;
            };
            let mut writeback_limit = self.update_writeback_limit(
                *zram,
                params,
                Some(limit_pages_4k),
                now,
                if idx == 0 { Some(&mut details) } else { None },
            )?;

            if per_device_params.huge_idle && writeback_limit.is_none_or(|limit| limit > 0) {
                writeback_limit = self.writeback::<Z, M>(
                    zram,
                    params,
                    Mode::HugeIdle,
                    suspend_history,
                    idle_marker,
                    &mut details.huge_idle,
                    now,
                )?;
            }
            if per_device_params.idle && writeback_limit.is_none_or(|limit| limit > 0) {
                writeback_limit = self.writeback::<Z, M>(
                    zram,
                    params,
                    Mode::Idle,
                    suspend_history,
                    idle_marker,
                    &mut details.idle,
                    now,
                )?;
            }
            if per_device_params.huge && writeback_limit.is_none_or(|limit| limit > 0) {
                self.writeback::<Z, M>(
                    zram,
                    params,
                    Mode::Huge,
                    suspend_history,
                    idle_marker,
                    &mut details.huge,
                    now,
                )?;
            }
            limit_pages_4k = limit_pages_4k.saturating_sub(details.huge_idle.written_pages_4k);
            limit_pages_4k = limit_pages_4k.saturating_sub(details.idle.written_pages_4k);
            limit_pages_4k = limit_pages_4k.saturating_sub(details.huge.written_pages_4k);
        }

        Ok(details)
    }

    // Calculates the limit of a single idle writeback operation based on
    // mmd.zram.writeback.min_bytes and mmd.zram.writeback.max_bytes.
    fn calculate_idle_writeback_limit(&self, params: &Params, stats: &Stats) -> u64 {
        let min_pages_4k = params.min_bytes / WRITEBACK_PAGE_SIZE;
        let max_pages_4k = params.max_bytes / WRITEBACK_PAGE_SIZE;
        // All calculations are performed in basis points, 100 bps = 1.00%. The number of pages
        // allowed to be written back follows a simple linear relationship. The allowable range is
        // [min_pages_4k, max_pages_4k], and the writeback limit will be the (zram utilization) * the
        // range, that is, the more zram we're using the more we're going to allow to be written
        // back.
        const BPS: u64 = 100 * 100;
        let zram_utilization_bps =
            stats.orig_data_size / WRITEBACK_PAGE_SIZE * BPS / self.total_zram_pages_4k;
        let limit_pages_4k = zram_utilization_bps * max_pages_4k / BPS;

        // And try to limit it to the approximate number of free backing device pages (if it's
        // less).
        let free_bd_pages_4k = self.zram_writeback_pages_4k - stats.current_writeback_pages_4k;
        let limit_pages_4k = std::cmp::min(limit_pages_4k, free_bd_pages_4k);

        if limit_pages_4k < min_pages_4k {
            // Configured to not writeback fewer than configured min_pages_4k.
            return 0;
        }

        // Finally enforce the limits, we won't even attempt writeback if we cannot writeback at
        // least the min, and we will cap to the max if it's greater.
        std::cmp::min(limit_pages_4k, max_pages_4k)
    }

    // TODO: b/408364803 - resolve clippy::too_many_arguments.
    #[allow(clippy::too_many_arguments)]
    fn writeback<Z: SysfsZramApi, M: MeminfoApi>(
        &mut self,
        zram: &Z,
        params: &Params,
        mode: Mode,
        suspend_history: &SuspendHistory,
        idle_marker: &dyn IdleMarker,
        details: &mut WritebackModeDetails,
        now: BootTime,
    ) -> Result<Option<u64>> {
        match mode {
            Mode::HugeIdle | Mode::Idle => {
                let idle_age = calculate_idle_time::<M>(params.min_idle, params.max_idle)?;
                // Adjust idle age by suspend duration.
                let idle_age = idle_age.saturating_add(
                    suspend_history.calculate_total_suspend_duration(idle_age, now),
                );
                match idle_marker.ensure_idle_pages_marked(zram, idle_age) {
                    Ok(true) => Ok(()),
                    Ok(false) => Err(Error::TryMarkIdleAgain),
                    Err(e) => Err(Error::MarkIdle(e)),
                }?;
            }
            Mode::Huge => {}
        }

        let mode = match mode {
            Mode::HugeIdle => "huge_idle",
            Mode::Idle => "idle",
            Mode::Huge => "huge",
        };

        let before = ZramBdStat::load(zram).map_err(Error::LoadStatsError)?;
        let result = zram.writeback(mode);
        let after = ZramBdStat::load(zram).map_err(Error::LoadStatsError)?;

        let written_pages_4k = after.bd_writes_pages_4k - before.bd_writes_pages_4k;
        details.written_pages_4k += written_pages_4k;
        self.history.record(details.written_pages_4k, now);

        let remaining_writeback_limit = load_effective_writeback_limit(zram, params);

        // Hitting the daily limit or running out of space aren't
        // real errors for system writeback, so don't propagate them.
        match result.map_err(|e| map_writeback_error(&remaining_writeback_limit, e)) {
            Ok(()) | Err(Error::WritebackDailyLimitExceeded) | Err(Error::WritebackNoSpace) => (),
            Err(e) => return Err(e),
        };

        self.last_writeback_at = Some(now);

        Ok(remaining_writeback_limit)
    }

    /// Does zram writeback for the given process.
    ///
    /// Returns false if writeback is already underway for the given process.
    pub fn writeback_process_zram_memory<'a, Z: SysfsZramApi + 'a>(
        &mut self,
        zram_devices: impl IntoIterator<Item = (&'a Z, &'a PerProcessIoctls)>,
        params: &Params,
        now: BootTime,
        pidfd: &OwnedFd,
        mut cur_state: PerProcessWbState,
        stop_signal: &AtomicBool,
    ) -> Result<(u64, Option<PerProcessWbState>)> {
        let mut total_written_bytes = 0;
        let zram_devices: Vec<_> = zram_devices.into_iter().collect();
        let num_devices = zram_devices.len();
        for (idx, (zram, per_process_ioctls)) in zram_devices.into_iter().enumerate() {
            let writeback_limit = self.update_writeback_limit(zram, params, None, now, None)?;
            if writeback_limit == Some(0) {
                return Err(Error::WritebackDailyLimitExceeded);
            }

            while idx == cur_state.cur_device_idx && !stop_signal.load(Ordering::Relaxed) {
                let writeback_result = per_process_ioctls.writeback(
                    pidfd,
                    cur_state.start_addr,
                    PER_PROCESS_WRITEBACK_MAX_OP_SIZE,
                );

                let writeback_result = match writeback_result {
                    Ok(writeback_result) => writeback_result,
                    Err(err) => {
                        if total_written_bytes != 0 {
                            self.history.record(total_written_bytes / WRITEBACK_PAGE_SIZE, now);
                        }
                        let remaining_writeback_limit =
                            load_effective_writeback_limit(zram, params);
                        return Err(map_writeback_error(&remaining_writeback_limit, err));
                    }
                };

                total_written_bytes += writeback_result.written_bytes;
                if writeback_result.next_addr == 0 {
                    cur_state.cur_device_idx += 1;
                    cur_state.start_addr = 0;
                } else {
                    cur_state.start_addr = writeback_result.next_addr;
                }
            }
        }
        self.history.record(total_written_bytes / WRITEBACK_PAGE_SIZE, now);
        if cur_state.cur_device_idx == num_devices {
            Ok((total_written_bytes, None))
        } else {
            Ok((total_written_bytes, Some(cur_state)))
        }
    }
}
