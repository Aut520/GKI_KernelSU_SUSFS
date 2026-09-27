// Copyright 2025, The Android Open Source Project
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

//! This module provides a rust API for zram's per-process ioctls.

use std::fs::OpenOptions;
use std::io;
use std::os::fd::AsRawFd;
use std::os::fd::OwnedFd;

mod internal {
    use nix::ioctl_none;
    use nix::ioctl_readwrite;
    use nix::ioctl_write_ptr;

    #[repr(C)]
    #[derive(Default)]
    pub struct zram_android_ioc_data_process_writeback {
        pub pidfd: u64,
        pub start_addr: u64,
        pub size: u64,
        pub next_addr: u64,
        pub written_bytes: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    pub struct zram_android_ioc_data_process_prefetch {
        pub pidfd: u64,
    }

    // The ioctl magic and command number
    const ZRAM_ANDROID_IOC_MAGIC: u8 = 0xBB;
    const ZRAM_ANDROID_IOC_PROCESS_WRITEBACK_CMD: u8 = 2;
    const ZRAM_ANDROID_IOC_PROCESS_PREFETCH_CMD: u8 = 3;
    const ZRAM_ANDROID_IOC_VERSION: u8 = 4;

    pub const ZRAM_ANDROID_IOC_CURRENT_VERSION: i32 = 1;

    ioctl_none!(zram_android_ioc_get_version, ZRAM_ANDROID_IOC_MAGIC, ZRAM_ANDROID_IOC_VERSION);

    ioctl_readwrite!(
        zram_android_ioc_process_writeback,
        ZRAM_ANDROID_IOC_MAGIC,
        ZRAM_ANDROID_IOC_PROCESS_WRITEBACK_CMD,
        zram_android_ioc_data_process_writeback
    );

    ioctl_write_ptr!(
        zram_android_ioc_process_prefetch,
        ZRAM_ANDROID_IOC_MAGIC,
        ZRAM_ANDROID_IOC_PROCESS_PREFETCH_CMD,
        zram_android_ioc_data_process_prefetch
    );
}

/// Struct that provides access to zram's per-process ioctls.
pub struct PerProcessIoctls {
    zram_fd: OwnedFd,
}

/// Result information from a per-process writeback request.
pub struct WritebackResult {
    /// The number of bytes written.
    pub written_bytes: u64,
    /// When per-process writeback stops early due to the size parameter, this is
    /// the address to use for the next per-process writeback request.
    pub next_addr: u64,
}

impl PerProcessIoctls {
    /// Construct a new PerProcessIoctls struct if supported by the kernel.
    pub fn try_new(idx: u64) -> io::Result<Option<PerProcessIoctls>> {
        let path = format!("/dev/block/zram{idx}");
        let zram = OpenOptions::new().read(true).write(true).open(path)?;
        let zram_fd: OwnedFd = zram.into();

        // SAFETY: The ioctl doesn't close the borrowed fd doesn't access memory.
        let version = unsafe { internal::zram_android_ioc_get_version(zram_fd.as_raw_fd()) };

        if version == Ok(internal::ZRAM_ANDROID_IOC_CURRENT_VERSION) {
            Ok(Some(PerProcessIoctls { zram_fd }))
        } else {
            Ok(None)
        }
    }

    /// Perform zram's per-process writeback operation on the process specified by pidfd.
    pub fn writeback(
        &self,
        pidfd: &OwnedFd,
        start_addr: u64,
        size: u64,
    ) -> io::Result<WritebackResult> {
        let mut args = internal::zram_android_ioc_data_process_writeback {
            pidfd: pidfd.as_raw_fd() as u64,
            start_addr,
            size,
            ..Default::default()
        };

        // SAFETY: The ioctl doesn't close the borrowed fd. args is the correct type,
        // valid for the duration of the call since it is local, and not used by
        // the kernel after the ioctl returns.
        let _ = unsafe {
            internal::zram_android_ioc_process_writeback(self.zram_fd.as_raw_fd(), &mut args)
        }?;

        Ok(WritebackResult { written_bytes: args.written_bytes, next_addr: args.next_addr })
    }

    /// Perform zram's per-process prefetch operation on the process specified by pidfd.
    pub fn prefetch(&self, pidfd: &OwnedFd) -> io::Result<()> {
        let args =
            internal::zram_android_ioc_data_process_prefetch { pidfd: pidfd.as_raw_fd() as u64 };

        // SAFETY: The ioctl doesn't close the borrowed fd. args is the correct type,
        // valid for the duration of the call since it is local, and not used by
        // the kernel after the ioctl returns.
        let _ = unsafe {
            internal::zram_android_ioc_process_prefetch(self.zram_fd.as_raw_fd(), &args)
        }?;
        Ok(())
    }
}
