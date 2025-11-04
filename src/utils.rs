// Copyright 2025 Irreducible Inc.

/// Creates a [`tracing`] event with the resident set size at its peak in megabytes.
///
/// The name of the event is `max rss mib`.
pub fn emit_max_rss() {
    #[cfg(unix)]
    let max_rss_mb = {
        use nix::sys::resource;

        resource::getrusage(resource::UsageWho::RUSAGE_SELF).map(|usage| {
            usage.max_rss()
                / if cfg!(target_os = "macos") {
                    // ... the result is in bytes on macOS
                    1024 * 1024
                } else {
                    // ... and in kilobytes on all other Unix systems
                    1024
                }
        })
    };

    #[cfg(windows)]
    let max_rss_mb = {
        use std::mem::MaybeUninit;
        use windows::Win32::System;

        let mut counters = MaybeUninit::uninit();

        unsafe {
            System::ProcessStatus::GetProcessMemoryInfo(
                System::Threading::GetCurrentProcess(),
                counters.as_mut_ptr(),
                size_of_val(&counters) as u32,
            )
            .map(|_| counters.assume_init().PeakWorkingSetSize / (1024 * 1024))
        }
    };

    if let Ok(max_rss_mb) = max_rss_mb {
        tracing::event!(
            name: "max rss mib",
            tracing::Level::INFO,
            value = max_rss_mb,
            counter = true
        );
    }
}
