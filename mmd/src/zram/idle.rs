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

//! This module provides the interface for CONFIG_ZRAM_MEMORY_TRACKING feature.

// #[cfg(test)]
// mod tests;

use std::error::Error;
use std::marker::PhantomData;
use std::time::Duration;

use crate::os::MeminfoApi;
use crate::time::BootTime;
use crate::time::TimeApi;
use crate::zram::SysfsZramApi;

/// IdleMarker marks pages in zram for longer than a specified time as idle.
///
/// libmmd provides [TrackedIdleMarker] and [UntrackedIdleMarker] for both system enabling
/// CONFIG_ZRAM_MEMORY_TRACKING or CONFIG_ZRAM_TRACK_ENTRY_ACTIME and system disabling them.
pub trait IdleMarker {
    /// Ensures that all zram pages marked as idle are older than `idle_age`.
    ///
    /// This returns `true` if all zram pages marked as idle are older than `idle_age`.
    ///
    /// If some zram pages are not eligible for writeback/recompress may be marked as idle, this
    /// returns `false`.
    fn ensure_idle_pages_marked(
        &self,
        zram: &dyn SysfsZramApi,
        idle_age: Duration,
    ) -> Result<bool, Box<dyn Error>>;
}

/// [TrackedIdleMarker] marks idle pages by writing specified idle duration to
/// "/sys/block/zram0/idle".
///
/// This requires CONFIG_ZRAM_MEMORY_TRACKING or CONFIG_ZRAM_TRACK_ENTRY_ACTIME kernel config is
/// enabled.
#[derive(Default, Debug)]
pub struct TrackedIdleMarker<Z: SysfsZramApi> {
    _phantom: PhantomData<Z>,
}

impl<Z: SysfsZramApi> TrackedIdleMarker<Z> {
    /// Creates a new [TrackedIdleMarker].
    pub fn new() -> Self {
        Self { _phantom: PhantomData }
    }
}

impl<Z: SysfsZramApi> IdleMarker for TrackedIdleMarker<Z> {
    /// Sets idle duration in seconds to "/sys/block/zram0/idle".
    ///
    /// Fractions of a second are truncated.
    fn ensure_idle_pages_marked(
        &self,
        zram: &dyn SysfsZramApi,
        idle_age: Duration,
    ) -> Result<bool, Box<dyn Error>> {
        match zram.set_idle(&idle_age.as_secs().to_string()) {
            Ok(()) => Ok(true),
            Err(e) => Err(Box::new(e)),
        }
    }
}

/// [UntrackedIdleMarker] marks all pages as idle and waits until specified time has passed.
///
/// The wait is unblocking and ensure_idle_pages_marked() just returns [MarkIdleResult::NotReady].
#[derive(Default, Debug)]
pub struct UntrackedIdleMarker<T: TimeApi> {
    last_marked_at: Option<BootTime>,
    _phantom: PhantomData<T>,
}

impl<T: TimeApi> UntrackedIdleMarker<T> {
    /// Creates a new [UntrackedIdleMarker].
    pub fn new() -> Self {
        Self { last_marked_at: None, _phantom: PhantomData }
    }

    /// Marks all pages as idle.
    ///
    /// [ensure_idle_pages_marked()] starts returning [MarkIdleResult::NotReady] until specified
    /// time has passed.
    pub fn refresh(&mut self, zram: &dyn SysfsZramApi) -> std::io::Result<()> {
        zram.set_idle("all")?;
        self.last_marked_at = Some(T::get_boot_time());
        Ok(())
    }
}

impl<T: TimeApi> IdleMarker for UntrackedIdleMarker<T> {
    fn ensure_idle_pages_marked(
        &self,
        _zram: &dyn SysfsZramApi,
        idle_age: Duration,
    ) -> Result<bool, Box<dyn Error>> {
        let Some(last_marked_at) = self.last_marked_at else {
            return Err(Box::<dyn Error>::from("last_marked_at is not set"));
        };
        let now = T::get_boot_time();
        Ok(now.saturating_duration_since(last_marked_at) >= idle_age)
    }
}

/// This parses the content of "/proc/meminfo" and returns the number of "MemTotal" and
/// "MemAvailable".
///
/// This does not care about the unit, because the user `calculate_idle_time()` use the values to
/// calculate memory utilization rate. The unit should be always "kB".
///
/// This returns `None` if this fails to parse the content.
fn parse_meminfo(content: &str) -> Option<(u64, u64)> {
    let mut total = None;
    let mut available = None;
    for line in content.split("\n") {
        let container = if line.contains("MemTotal:") {
            &mut total
        } else if line.contains("MemAvailable:") {
            &mut available
        } else {
            continue;
        };
        let Some(number_str) = line.split_whitespace().nth(1) else {
            continue;
        };
        let Ok(number) = number_str.parse::<u64>() else {
            continue;
        };
        *container = Some(number);
    }
    if let (Some(total), Some(available)) = (total, available) {
        Some((total, available))
    } else {
        None
    }
}

/// Error from [calculate_idle_time].
#[derive(Debug, thiserror::Error)]
pub enum CalculateError {
    /// min_idle is longer than max_idle
    #[error("min_idle is longer than max_idle")]
    InvalidMinAndMax,
    /// failed to parse meminfo
    #[error("failed to parse meminfo")]
    InvalidMeminfo,
    /// failed to read meminfo
    #[error("failed to read meminfo: {0}")]
    ReadMeminfo(std::io::Error),
}

/// Calculates idle duration from min_idle and max_idle using meminfo.
pub fn calculate_idle_time<M: MeminfoApi>(
    min_idle: Duration,
    max_idle: Duration,
) -> std::result::Result<Duration, CalculateError> {
    if min_idle > max_idle {
        return Err(CalculateError::InvalidMinAndMax);
    }
    let content = match M::read_meminfo() {
        Ok(v) => v,
        Err(e) => return Err(CalculateError::ReadMeminfo(e)),
    };
    let (total, available) = match parse_meminfo(&content) {
        Some((total, available)) if total > 0 => (total, available),
        _ => {
            // Fallback to use the safest value.
            return Err(CalculateError::InvalidMeminfo);
        }
    };

    let mem_utilization = 1.0 - (available as f64) / (total as f64);

    // Exponentially decay the age vs. memory utilization. The reason we choose exponential decay is
    // because we want to do as little work as possible when the system is under very low memory
    // pressure. As pressure increases we want to start aggressively shrinking our idle age to force
    // newer pages to be written back/recompressed.
    const LAMBDA: f64 = 5.0;
    let seconds = ((max_idle - min_idle).as_secs() as f64)
        * std::f64::consts::E.powf(-LAMBDA * mem_utilization)
        + (min_idle.as_secs() as f64);

    Ok(Duration::from_secs(seconds as u64))
}
