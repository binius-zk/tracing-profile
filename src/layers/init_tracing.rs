// Copyright 2024-2025 Irreducible Inc.

use cfg_if::cfg_if;
use thiserror::Error;
use tracing::{level_filters::LevelFilter, Subscriber};
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::{
    filter::Filtered,
    layer::SubscriberExt,
    util::{SubscriberInitExt, TryInitError},
    Layer,
};

use crate::{PrintTreeConfig, PrintTreeLayer};

trait WithEnvFilter<S: Subscriber>: Layer<S> + Sized {
    fn with_env_filter(self) -> Filtered<Self, EnvFilter, S> {
        let env_level_filter = EnvFilter::builder()
            .with_default_directive(LevelFilter::DEBUG.into())
            .from_env_lossy();

        self.with_filter(env_level_filter)
    }
}

impl<S: Subscriber, T: Layer<S>> WithEnvFilter<S> for T {}

#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to initialize tracing: {0}")]
    TryInit(#[from] TryInitError),
    #[cfg(feature = "perfetto")]
    #[error("failed to initialize Perfetto: {0}")]
    Perfetto(#[from] perfetto_sys::Error),
    #[cfg(feature = "perf_counters")]
    #[error("failed to initialize PerfCounters: {0}")]
    PerfCounters(#[from] std::io::Error),
    #[cfg(feature = "gen_filename")]
    #[error("failed to initialize filename builder: {0}")]
    FilenameBuilder(#[from] crate::filename_builder::FilenameBuilderError),
}

// Type aliases to handle conditional compilation cleanly
#[cfg(feature = "gen_filename")]
mod filename_support {
    pub use crate::filename_builder::TraceFilenameBuilder;
    pub type BuilderOption = Option<TraceFilenameBuilder>;
}

#[cfg(not(feature = "gen_filename"))]
mod filename_support {
    pub type BuilderOption = Option<()>;
}

use filename_support::BuilderOption;

/// Internal function that handles common layer setup logic.
///
/// Sets up all available tracing layers based on enabled features:
/// - PrintTreeLayer (always enabled)
/// - PerfettoLayer (if perfetto feature enabled)
/// - IttApiLayer (if ittapi feature enabled)
/// - PrintPerfCountersLayer (if perf_counters feature enabled)
///
/// The builder parameter allows customization of perfetto trace filenames
/// when the gen_filename feature is enabled.
fn init_tracing_internal(_builder: BuilderOption) -> Result<impl Drop, Error> {
    // Create print tree layer
    let (layer, guard) = PrintTreeLayer::new(PrintTreeConfig::default());
    let layer = tracing_subscriber::registry().with(layer.with_env_filter());

    // Add perfetto layer if feature is enabled
    let (layer, guard) = {
        cfg_if! {
            if #[cfg(feature = "perfetto")] {
                let (new_layer, new_guard) = match _builder {
                    None => {
                        crate::PerfettoLayer::new_from_env()?
                    }
                    Some(builder) => {
                        crate::PerfettoLayer::new_from_env_with_builder(builder)
                            .map_err(Error::Perfetto)?
                    }
                };
                (layer.with(new_layer.with_env_filter()), crate::data::GuardWrapper::wrap(guard, new_guard))
            } else {
                (layer, guard)
            }
        }
    };

    // Add ITT API layer if feature is enabled
    let (layer, guard) = {
        cfg_if! {
            if #[cfg(feature = "ittapi")] {
                (layer.with(crate::IttApiLayer::new().with_env_filter()), guard)
            } else {
                (layer, guard)
            }
        }
    };

    // Add perf counters layer if feature is enabled
    let (layer, guard) = {
        cfg_if! {
            if #[cfg(feature = "perf_counters")] {
                (layer.with(
                    crate::PrintPerfCountersLayer::new(vec![
                        ("instructions".to_string(), crate::PerfHardwareEvent::INSTRUCTIONS.into()),
                        ("cycles".to_string(), crate::PerfHardwareEvent::CPU_CYCLES.into()),
                    ])?
                    .with_env_filter(),
                ), guard)
            } else {
                (layer, guard)
            }
        }
    };

    // Try to initialize subscriber - OK if already set
    match layer.try_init() {
        Ok(()) => {
            // First initialization succeeded
        }
        Err(_) => {
            // Subscriber already initialized - this is fine
            // The perfetto guard will still be created and work correctly
            // Tracing events will go to the existing subscriber
        }
    }

    Ok(guard)
}

/// Initialize the tracing with the default values depending on the features enabled and environment variables set.
///
/// The following layers are added:
/// - `PrintTreeLayer` (added always)
/// - `IttApiLayer` (added if feature `ittapi` is enabled)
/// - `PrintPerfCountersLayer` (added if feature `perf_counters` is enabled)
///
/// Returns the guard that should be kept alive for the duration of the program.
pub fn init_tracing() -> Result<impl Drop, Error> {
    init_tracing_internal(None)
}

/// Initialize tracing with a custom TraceFilenameBuilder for perfetto traces.
///
/// This function provides the same functionality as `init_tracing()` but allows
/// you to customize the perfetto trace filename using TraceFilenameBuilder.
///
/// # Example
/// ```rust,no_run
/// use tracing_profile::{init_tracing_with_builder, TraceFilenameBuilder};
///
/// let builder = TraceFilenameBuilder::new()
///     .name("my_app")
///     .iteration(1)
///     .timestamp()
///     .git_info();
///
/// let _guard = init_tracing_with_builder(builder).unwrap();
/// ```
#[cfg(feature = "gen_filename")]
pub fn init_tracing_with_builder(
    builder: crate::filename_builder::TraceFilenameBuilder,
) -> Result<impl Drop, Error> {
    init_tracing_internal(Some(builder))
}
