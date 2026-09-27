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

//! This module provides policies to manage zram features.

pub mod idle;
pub mod per_process_ioctls;
pub mod recompression;
pub mod setup;
pub mod stats;
pub mod writeback;

use std::io;
use std::path::Path;
use std::path::PathBuf;

use nix::sys::stat::FchmodatFlags;
use nix::sys::stat::Mode;
use nix::unistd::User;

/// [SysfsZramApi] is a mockable interface for access to files under
/// "/sys/block/zramX".
///
/// The naming convention: functions for files which is readable and writable
///
/// * fn read_<file_name>() -> io::Result<String>
/// * fn write_<file_name>(contents: &str) -> io::Result<()>
///
/// We don't have naming conventions for files which is writable only.
pub trait SysfsZramApi: Send {
    /// Gets the index value X of zramX.
    fn idx(&self) -> u64;

    /// Read "/sys/block/zramX/disksize".
    fn read_disksize(&self) -> io::Result<String>;
    /// Write "/sys/block/zramX/disksize".
    fn write_disksize(&self, contents: &str) -> io::Result<()>;
    /// Read "/sys/block/zramX/mm_stat".
    fn read_mm_stat(&self) -> io::Result<String>;

    /// Read "/sys/block/zramX/comp_algorithm"
    fn read_comp_algorithm(&self) -> io::Result<String>;
    /// Set compression algorithm.
    fn write_comp_algorithm(&self, contents: &str) -> io::Result<()>;

    /// Write contents to "/sys/block/zramX/idle".
    fn set_idle(&self, contents: &str) -> io::Result<()>;

    /// Read "/sys/block/zramX/backing_dev".
    fn read_backing_dev(&self) -> io::Result<String>;
    /// Write "/sys/block/zramX/backing_dev".
    fn write_backing_dev(&self, contents: &str) -> io::Result<()>;
    /// Write contents to "/sys/block/zramX/writeback".
    fn writeback(&self, contents: &str) -> io::Result<()>;
    /// Write contents to "/sys/block/zramX/writeback_limit_enable".
    fn write_writeback_limit_enable(&self, contents: &str) -> io::Result<()>;
    /// Write contents to "/sys/block/zramX/writeback_limit".
    fn write_writeback_limit(&self, contents: &str) -> io::Result<()>;
    /// Read "/sys/block/zramX/writeback_limit".
    fn read_writeback_limit(&self) -> io::Result<String>;
    /// Write contents to "/sys/block/zramX/compressed_writeback".
    fn write_compressed_writeback(&self, contents: &str) -> io::Result<()>;
    /// Read "/sys/block/zramX/compressed_writeback".
    fn read_compressed_writeback(&self) -> io::Result<String>;
    /// Read "/sys/block/zramX/bd_stat".
    fn read_bd_stat(&self) -> io::Result<String>;

    /// Read "/sys/block/zramX/recomp_algorithm".
    fn read_recomp_algorithm(&self) -> io::Result<String>;
    /// Write "/sys/block/zramX/recomp_algorithm".
    fn write_recomp_algorithm(&self, contents: &str) -> io::Result<()>;
    /// Write contents to "/sys/block/zramX/recompress".
    fn recompress(&self, contents: &str) -> io::Result<()>;

    /// Read "/sys/block/zramX/io_stat".
    fn read_io_stat(&self) -> io::Result<String>;

    /// Read "/sys/block/zramX/max_comp_streams".
    fn read_max_comp_streams(&self) -> io::Result<String>;
}

/// The implementation of [SysfsZramApi].
pub struct SysfsZramApiImpl {
    idx: u64,
    root_path: PathBuf,
}

impl SysfsZramApiImpl {
    /// Creates a new SysfsZramApiImpl for /sys/block/zram{idx}.
    pub fn new(idx: u64) -> SysfsZramApiImpl {
        Self { idx, root_path: PathBuf::from(format!("/sys/block/zram{}", idx)) }
    }

    /// Sets up permissions on zram related files.
    pub fn setup_permissions(&self) -> io::Result<()> {
        let mmd = User::from_name("mmd")?.ok_or(io::Error::other("no mmd"))?;
        let system = User::from_name("system")?.ok_or(io::Error::other("no system"))?;
        let permission_updates = [
            (self.root_path.join("recompress"), &mmd, Mode::from_bits_truncate(0o220)),
            (self.root_path.join("writeback_limit"), &mmd, Mode::from_bits_truncate(0o664)),
            (self.root_path.join("idle"), &system, Mode::from_bits_truncate(0o220)),
            (self.root_path.join("writeback"), &system, Mode::from_bits_truncate(0o220)),
            // To do per-process writeback, mmd needs read/write permissions to the
            // zram device to issue the ioctl. Seccomp is used to restrict mmd to only
            // ioctls and prevent reading/writing arbitrary swap data.
            (
                PathBuf::from(format!("/dev/block/zram{}", self.idx)),
                &mmd,
                Mode::from_bits_truncate(0o660),
            ),
        ];

        for (path, uuid, mode) in permission_updates.iter() {
            let path = Path::new(path);
            if !path.exists() {
                continue;
            }

            nix::unistd::chown(path, None, Some(uuid.gid))?;
            nix::sys::stat::fchmodat(None, path, *mode, FchmodatFlags::FollowSymlink)?;
        }

        Ok(())
    }
}

impl SysfsZramApi for SysfsZramApiImpl {
    fn idx(&self) -> u64 {
        self.idx
    }

    fn read_disksize(&self) -> io::Result<String> {
        std::fs::read_to_string(self.root_path.join("disksize"))
    }

    fn write_disksize(&self, contents: &str) -> io::Result<()> {
        std::fs::write(self.root_path.join("disksize"), contents)
    }

    fn read_mm_stat(&self) -> io::Result<String> {
        std::fs::read_to_string(self.root_path.join("mm_stat"))
    }

    fn set_idle(&self, contents: &str) -> io::Result<()> {
        std::fs::write(self.root_path.join("idle"), contents)
    }

    fn read_backing_dev(&self) -> io::Result<String> {
        std::fs::read_to_string(self.root_path.join("backing_dev"))
    }

    fn write_backing_dev(&self, contents: &str) -> io::Result<()> {
        std::fs::write(self.root_path.join("backing_dev"), contents)
    }

    fn writeback(&self, contents: &str) -> io::Result<()> {
        std::fs::write(self.root_path.join("writeback"), contents)
    }

    fn write_writeback_limit(&self, contents: &str) -> io::Result<()> {
        std::fs::write(self.root_path.join("writeback_limit"), contents)
    }

    fn write_writeback_limit_enable(&self, contents: &str) -> io::Result<()> {
        std::fs::write(self.root_path.join("writeback_limit_enable"), contents)
    }

    fn read_writeback_limit(&self) -> io::Result<String> {
        std::fs::read_to_string(self.root_path.join("writeback_limit"))
    }

    fn read_compressed_writeback(&self) -> io::Result<String> {
        std::fs::read_to_string(self.root_path.join("compressed_writeback"))
    }

    fn write_compressed_writeback(&self, contents: &str) -> io::Result<()> {
        std::fs::write(self.root_path.join("compressed_writeback"), contents)
    }

    fn read_bd_stat(&self) -> io::Result<String> {
        std::fs::read_to_string(self.root_path.join("bd_stat"))
    }

    fn read_recomp_algorithm(&self) -> io::Result<String> {
        std::fs::read_to_string(self.root_path.join("recomp_algorithm"))
    }

    fn write_recomp_algorithm(&self, contents: &str) -> io::Result<()> {
        std::fs::write(self.root_path.join("recomp_algorithm"), contents)
    }

    fn recompress(&self, contents: &str) -> io::Result<()> {
        std::fs::write(self.root_path.join("recompress"), contents)
    }

    fn read_comp_algorithm(&self) -> io::Result<String> {
        std::fs::read_to_string(self.root_path.join("comp_algorithm"))
    }

    fn write_comp_algorithm(&self, contents: &str) -> io::Result<()> {
        std::fs::write(self.root_path.join("comp_algorithm"), contents)
    }

    fn read_io_stat(&self) -> io::Result<String> {
        std::fs::read_to_string(self.root_path.join("io_stat"))
    }

    fn read_max_comp_streams(&self) -> io::Result<String> {
        std::fs::read_to_string(self.root_path.join("max_comp_streams"))
    }
}


// The default minimum size a zram writeback device may be.
// This prevents a writeback device of 1MiB from being created, for example.
const DEFAULT_WRITEBACK_MIN_VOLUME_SIZE: u64 = 128 << 20; // 128 MiB.

/// Calculates the final writeback device size based on requested parameters and constraints.
pub fn adjust_writeback_device_size(
    requested_device_size: u64,
    min_free_space: u64,
    free_space: u64,
    block_size: u64,
) -> u64 {
    if free_space <= min_free_space {
        return 0;
    }

    let mut adjusted_device_size = if requested_device_size + min_free_space > free_space {
        free_space - min_free_space
    } else {
        requested_device_size
    };

    if adjusted_device_size < DEFAULT_WRITEBACK_MIN_VOLUME_SIZE {
        return 0;
    }

    if adjusted_device_size % block_size != 0 {
        adjusted_device_size = adjusted_device_size - adjusted_device_size % block_size;
    }
    adjusted_device_size
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1 << 20;
    const GIB: u64 = 1 << 30;
    const DEFAULT_BLOCK_SIZE: u64 = 4096;

    #[test]
    fn adjust_writeback_device_size_enough_disk_space() {
        let size = adjust_writeback_device_size(
            /* requested_device_size */ GIB,
            /* min_free_space */ GIB,
            /* free_space */ 3 * GIB,
            DEFAULT_BLOCK_SIZE,
        );
        assert_eq!(size, GIB);
    }

    #[test]
    fn adjust_writeback_device_size_enough_disk_space_but_size_too_small() {
        let size = adjust_writeback_device_size(
            /* requested_device_size */ 127 * MIB,
            /* min_free_space */ GIB,
            /* free_space */ 3 * GIB,
            DEFAULT_BLOCK_SIZE,
        );
        assert_eq!(size, 0);
    }

    #[test]
    fn adjust_writeback_device_size_enough_disk_space_meeting_min_size_requirement() {
        let size = adjust_writeback_device_size(
            /* requested_device_size */ 128 * MIB,
            /* min_free_space */ GIB,
            /* free_space */ 3 * GIB,
            DEFAULT_BLOCK_SIZE,
        );
        assert_eq!(size, 128 * MIB);
    }

    #[test]
    fn adjust_writeback_device_size_disk_space_too_low() {
        let size = adjust_writeback_device_size(
            /* requested_device_size */ GIB,
            /* min_free_space */ 2 * GIB,
            /* free_space */ GIB,
            DEFAULT_BLOCK_SIZE,
        );
        assert_eq!(size, 0);
    }

    #[test]
    fn adjust_writeback_device_size_needs_adjusted() {
        let size = adjust_writeback_device_size(
            /* requested_device_size */ 2 * GIB,
            /* min_free_space */ GIB,
            /* free_space */ 2 * GIB,
            DEFAULT_BLOCK_SIZE,
        );
        assert_eq!(size, GIB);
    }

    #[test]
    fn adjust_writeback_device_size_too_small_after_adjusted() {
        let size = adjust_writeback_device_size(
            /* requested_device_size */ 2 * GIB,
            /* min_free_space */ GIB,
            /* free_space */ GIB + MIB,
            DEFAULT_BLOCK_SIZE,
        );
        assert_eq!(size, 0);
    }

    #[test]
    fn adjust_writeback_device_size_block_size_alignment() {
        let size = adjust_writeback_device_size(
            /* requested_device_size */ GIB + 1,
            /* min_free_space */ GIB,
            /* free_space */ 3 * GIB,
            DEFAULT_BLOCK_SIZE,
        );
        assert_eq!(size, GIB);
    }
}
