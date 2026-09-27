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

//! This module implements zram setup functionality.
//!
//! The setup implemented in this module assumes that the zram kernel module has been loaded early on init with only 1 zram device (`zram0`).
//!
//! zram kernel documentation https://docs.kernel.org/admin-guide/blockdev/zram.html

// #[cfg(test)]
// mod tests;

use std::fs::File;
use std::fs::Permissions;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug)]
pub struct LoopDevice {
    pub path: PathBuf,
}

use crate::os::fallocate;
use crate::zram::SysfsZramApi;

const MKSWAP_BIN_PATH: &str = "/system/bin/mkswap";
const PROC_SWAPS_PATH: &str = "/proc/swaps";

// Constants not defined in rust's libc, copied from bionic instead.
const SWAP_FLAG_PREFER: i32 = 0x8000;
const SWAP_FLAG_PRIO_MASK: i32 = 0x7fff;
const SWAP_FLAG_PRIO_SHIFT: i32 = 0;

pub trait SetupApi {
    /// Set up zram swap device, returning whether the command succeeded and its output.
    fn mkswap(device_path: &str) -> io::Result<std::process::Output>;
    /// Specify the zram swap device.
    fn swapon(device_path: &std::ffi::CStr, priority: Option<u64>) -> io::Result<()>;
    /// Read swaps areas in use.
    fn read_swap_areas() -> io::Result<String>;
    /// Set up a new loop device for a backing file with size.
    fn attach_loop_device(
        file_path: &Path,
        device_size: u64,
    ) -> anyhow::Result<LoopDevice>;
}

/// The implementation of [SetupApi].
pub struct SetupApiImpl;

impl SetupApi for SetupApiImpl {
    fn mkswap(device_path: &str) -> io::Result<std::process::Output> {
        std::process::Command::new(MKSWAP_BIN_PATH).arg(device_path).output()
    }

    fn swapon(device_path: &std::ffi::CStr, priority: Option<u64>) -> io::Result<()> {
        let flags = priority.map_or(0, |p| {
            SWAP_FLAG_PREFER | (((p as i32) & SWAP_FLAG_PRIO_MASK) << SWAP_FLAG_PRIO_SHIFT)
        });
        // SAFETY: device_path is a nul-terminated string.
        let res = unsafe { libc::swapon(device_path.as_ptr(), flags) };
        if res == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    fn read_swap_areas() -> io::Result<String> {
        std::fs::read_to_string(PROC_SWAPS_PATH)
    }

    fn attach_loop_device(
        file_path: &Path,
        _device_size: u64,
    ) -> anyhow::Result<LoopDevice> {
        let output = std::process::Command::new("losetup")
            .arg("-f")
            .arg("--show")
            .arg(file_path)
            .output()?;
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(LoopDevice { path: PathBuf::from(path_str) })
        } else {
            Err(anyhow::anyhow!("losetup failed: {:?}", output))
        }
    }
}



/// Whether or not zram is already set up on the device.
pub fn is_zram_swap_activated<S: SetupApi>() -> io::Result<bool> {
    let swaps = S::read_swap_areas()?;
    // Skip the first line which is header.
    let swap_lines = swaps.lines().skip(1);
    // Swap is turned on if swap file contains entry with zram keyword.
    for line in swap_lines {
        if line.contains("zram") {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Error from [activate].
#[derive(Debug, thiserror::Error)]
pub enum ZramActivationError {
    /// Failed to update zram disk size
    #[error("failed to write zram disk size: {0}")]
    UpdateZramDiskSize(std::io::Error),
    /// Failed to swapon
    #[error("swapon failed: {0}")]
    SwapOn(std::io::Error),
    /// Mkswap command failed
    #[error("failed to execute mkswap: {0}")]
    ExecuteMkSwap(std::io::Error),
    /// Mkswap command failed
    #[error("mkswap failed: {0:?}")]
    MkSwap(std::process::Output),
}

/// Set up a zram device with provided parameters.
pub fn activate_zram<Z: SysfsZramApi, S: SetupApi>(
    zram: &Z,
    zram_size: u64,
    priority: Option<u64>,
) -> Result<(), ZramActivationError> {
    zram.write_disksize(&zram_size.to_string()).map_err(ZramActivationError::UpdateZramDiskSize)?;
    let path = format!("/dev/block/zram{}", zram.idx());
    let output = S::mkswap(&path).map_err(ZramActivationError::ExecuteMkSwap)?;
    if !output.status.success() {
        return Err(ZramActivationError::MkSwap(output));
    }

    let zram_device_path_cstring =
        std::ffi::CString::new(path).expect("device path should have no nul characters");
    S::swapon(&zram_device_path_cstring, priority).map_err(ZramActivationError::SwapOn)?;

    Ok(())
}

/// Error from [create_zram_writeback_device].
#[derive(Debug, thiserror::Error)]
pub enum WritebackDeviceSetupError {
    /// Failed to create backing file
    #[error("failed to create backing file: {0}")]
    CreateBackingFile(std::io::Error),
    /// Failed to create the backing device
    #[error("failed to create backing device: {0}")]
    CreateBackingDevice(anyhow::Error),
}

/// Create a zram backing device with provided file path and size.
pub fn create_zram_writeback_device<S: SetupApi>(
    file_path: &Path,
    device_size: u64,
) -> std::result::Result<LoopDevice, WritebackDeviceSetupError> {
    let swap_file =
        File::create(file_path).map_err(WritebackDeviceSetupError::CreateBackingFile)?;
    scopeguard::defer! {
        let _ = std::fs::remove_file(file_path);
    }
    std::fs::set_permissions(file_path, Permissions::from_mode(0o600))
        .map_err(WritebackDeviceSetupError::CreateBackingFile)?;

    fallocate(&swap_file, device_size).map_err(WritebackDeviceSetupError::CreateBackingFile)?;

    let loop_device = S::attach_loop_device(file_path, device_size)
        .map_err(WritebackDeviceSetupError::CreateBackingDevice)?;

    Ok(loop_device)
}

/// Enables zram writeback limit.
pub fn enable_zram_writeback_limit<Z: SysfsZramApi>(zram: &Z) -> std::io::Result<()> {
    zram.write_writeback_limit_enable("1")
}

/// Gets supported compression algorithms
pub fn get_supported_compression_algorithms<Z: SysfsZramApi>(
    zram: &Z,
) -> std::io::Result<Vec<String>> {
    let contents = zram.read_comp_algorithm()?;
    let supported_algorithms = contents.split_whitespace().map(|s|
        // Selected algorithm is in square brackets
        if s.starts_with("[") && s.ends_with("]") {
            s[1..s.len() - 1].to_string()
        } else {
            s.to_string()
        }
    ).collect();
    Ok(supported_algorithms)
}
