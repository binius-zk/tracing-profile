// Copyright 2024-2025 Irreducible Inc.

//! A span based profiler, utilizing the [tracing](https://docs.rs/tracing/latest/tracing/) crate.
//!
//! # Overview
//! This implementation of `tracing_subscriber::Layer<S>` records the time
//! a span took to execute, along with any user supplied metadata and
//! information necessary to construct a call graph from the resulting logs.
//!
//! Multiple `Layer` implementations are provided:
//!     `PrintTreeLayer`: prints a call graph
//!     `PerfettoLayer`: uses a local or system-wide perfetto tracing service to record data.
//!     `IttApiLayer`: logs data in Intel's [ITT API](https://www.intel.com/content/www/us/en/docs/vtune-profiler/user-guide/2023-1/instrumentation-and-tracing-technology-apis.html)
//!
//! `init_tracing` is a convenience function that initializes the tracing with the default values
//! depending on the features enabled and environment variables set.
//!
//! For advanced filename customization, use `init_tracing_with_builder` with `TraceFilenameBuilder`.
//!
//! ## Basic Usage
//!
//! ```no_run
//! use tracing::instrument;
//! use tracing::debug_span;
//! use tracing_profile::init_tracing;
//!
//! #[instrument(skip_all, name= "graph_root", fields(a="b", c="d"))]
//! fn entry_point() {
//!     let span = debug_span!("some_span");
//!     let _scope1 = span.enter();
//!
//!     let span2 = debug_span!("another_span", field1 = "value1");
//!     let _scope2 = span2.enter();
//! }
//!
//! fn main() {
//!     // Initialize the tracing with the default values
//!     // Note that the guard must be kept alive for the duration of the program.
//!     let _guard = init_tracing().unwrap();
//!
//!     entry_point();
//! }
//! ```
//!
//! ## Advanced Usage with Custom Filenames
//!
//! With the `gen_filename` feature enabled, you can use a builder pattern:
//!
//! ```ignore
//! use tracing_profile::{init_tracing_with_builder, TraceFilenameBuilder};
//!
//! // Create a custom filename builder
//! let builder = TraceFilenameBuilder::new()
//!     .name("my_application")
//!     .iteration(1)
//!     .timestamp()
//!     .git_info()
//!     .platform()
//!     .hostname();
//!
//! // Initialize tracing with the custom builder
//! let _guard = init_tracing_with_builder(builder).unwrap();
//!
//! // Your application code here...
//! ```
//!
//! Note that if `#[instrument]` is used, `skip_all` is recommended. Omitting this will result in
//! all the function arguments being included as fields.
//!
//! # Features
//! The `panic` feature will turn eprintln! into panic!, causing the program to halt on errors.

mod data;
mod env_utils;
mod errors;
#[cfg(feature = "gen_filename")]
pub mod filename_builder;
#[cfg(feature = "gen_filename")]
mod filename_utils;
mod layers;
pub mod utils;

pub use layers::graph::{Config as PrintTreeConfig, Layer as PrintTreeLayer};
#[cfg(feature = "ittapi")]
pub use layers::ittapi::Layer as IttApiLayer;
#[cfg(feature = "perfetto")]
pub use layers::perfetto::{Layer as PerfettoLayer, PerfettoSettings as PerfettoCategorySettings};
#[cfg(feature = "perfetto")]
pub use perfetto_sys::PerfettoGuard;

#[cfg(feature = "gen_filename")]
pub use filename_builder::{FilenameBuilderError, TraceFilenameBuilder};

pub use layers::init_tracing::init_tracing;
#[cfg(feature = "gen_filename")]
pub use layers::init_tracing::init_tracing_with_builder;

/// Test utilities for handling Perfetto test directory setup and cleanup.
///
/// **Internal Testing API**: This module is intended only for internal testing
/// and should not be used by external consumers of this crate. The API may
/// change without notice.
///
/// Note: Uses `#[doc(hidden)]` instead of `#[cfg(test)]` to enable access from
/// integration tests while keeping it out of public documentation.
#[doc(hidden)]
pub mod test_utils {
    /// RAII guard for Perfetto test directory management.
    ///
    /// Sets up temporary directory on creation, cleans up on drop (even if tests panic).
    pub struct PerfettoTestDir {
        path: String,
    }

    impl Default for PerfettoTestDir {
        fn default() -> Self {
            Self::new()
        }
    }

    impl PerfettoTestDir {
        /// Creates a new test directory guard.
        pub fn new() -> Self {
            use std::env;
            let temp_dir = env::temp_dir().join("tracing_profile_tests");
            std::fs::create_dir_all(&temp_dir).ok();
            let temp_path = temp_dir.to_string_lossy().to_string();
            env::set_var("PERFETTO_TRACE_DIR", &temp_path);
            Self { path: temp_path }
        }

        /// Returns the path to the test directory
        pub fn path(&self) -> &str {
            &self.path
        }
    }

    impl Drop for PerfettoTestDir {
        /// Automatically cleans up the test directory and environment variables
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.path).ok();
            std::env::remove_var("PERFETTO_TRACE_DIR");
            // Also clean up any files that might have been created in current dir before we set the env var
            let _ = std::fs::remove_file(".last_perfetto_trace_path");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::Duration;

    use rusty_fork::rusty_fork_test;
    use tracing::{debug_span, event, Level};

    use super::*;
    use crate::test_utils::PerfettoTestDir;

    fn make_spans() {
        event!(name: "event outside of span", Level::DEBUG, {value = 10});
        event!(name: "test_instant_event", Level::DEBUG, test_key = "test_value");

        {
            let span = debug_span!("root span");
            let _scope1 = span.enter();
            thread::sleep(Duration::from_millis(20));

            // child spans 1 and 2 are siblings
            let span2 = debug_span!("child span1", field1 = "value1", perfetto_track_id = 5);
            let scope2 = span2.enter();
            thread::sleep(Duration::from_millis(20));
            drop(scope2);

            let span3 = debug_span!(
                "child span2",
                field2 = "value2",
                value = 20,
                perfetto_track_id = 5,
                perfetto_flow_id = 10
            );
            let _scope3 = span3.enter();

            thread::sleep(Duration::from_millis(20));
            event!(name: "event in span2", Level::DEBUG, {value = 100});

            // child spans 3 and 4 are siblings
            let span = debug_span!("child span3", field3 = "value3");
            let scope = span.enter();
            thread::sleep(Duration::from_millis(20));
            event!(name: "custom event", Level::DEBUG, {field5 = "value5", counter = true, value = 30});
            drop(scope);

            thread::spawn(|| {
                let span = debug_span!("child span5", field5 = "value5");
                let _scope = span.enter();
                thread::sleep(Duration::from_millis(20));
                event!(name: "custom event", Level::DEBUG, {field5 = "value6", counter = true, value = 10});
            }).join().unwrap();

            let span = debug_span!("child span4", field4 = "value4", perfetto_flow_id = 10);
            thread::sleep(Duration::from_millis(20));
            event!(name: "custom event", Level::DEBUG, {field5 = "value5", counter = true, value = 40});
            let scope = span.enter();
            thread::sleep(Duration::from_millis(20));
            drop(scope);
        }
        event!(name: "event after last span", Level::DEBUG, {value = 20});
    }

    // Since tracing_subscriber::registry() is a global singleton, we need to run the tests in separate processes.
    rusty_fork_test! {
        #[test]
        fn all_layers() {
            let _test_dir = PerfettoTestDir::new();
            let _guard = init_tracing().unwrap();

            make_spans();
        }
    }
}
