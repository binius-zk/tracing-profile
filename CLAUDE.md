# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

### Build
```bash
cargo build                       # Build the library
cargo build --all-features       # Build with all features enabled
```

### Test
```bash
cargo test                        # Run all tests
cargo test -- --nocapture        # Run tests with output visible
cargo test <test_name>           # Run a specific test
```

### Lint and Format
```bash
cargo fmt                         # Format code
cargo fmt -- --check             # Check if code is formatted (CI uses this)
cargo clippy --all-features --tests --examples --benches -- -D warnings  # Run clippy with all checks (CI requirement)
```

### Copyright Check
```bash
./scripts/check_copyright_notice.sh  # Check copyright notices in source files
```

## Architecture

This is a Rust library implementing profiling layers for the `tracing` crate. The architecture consists of:

### Core Components

1. **Layers** (`src/layers/`): Different profiling output implementations
   - `PrintTreeLayer`: Prints hierarchical span timing information
   - `PrintPerfCountersLayer`: Aggregates and prints performance counters (Linux only, requires `perf_counters` feature)
   - `PerfettoLayer`: Integrates with Perfetto tracing system (Linux, macOS, and Android, requires `perfetto` feature)
   - `IttApiLayer`: Intel VTune integration (requires `ittapi` feature)

2. **Data Structures** (`src/data/`): Core data management
   - Span metadata tracking and storage
   - Event counting and aggregation
   - Tree structure for span hierarchies
   - Field visitor for extracting span attributes

3. **Perfetto System Bindings** (`perfetto-sys/`): Separate crate providing low-level C++ bindings to Perfetto SDK

### Key Design Patterns

- Each layer implements `tracing_subscriber::Layer<S>` trait
- Layers track span timing using `on_new_span`, `on_enter`, `on_exit`, and `on_close` callbacks
- Span relationships are tracked to build call graphs
- Thread-local storage is used for performance counter tracking
- Guards are used to ensure proper cleanup (e.g., `PerfettoGuard`)

### Feature Flags

- `perf_counters`: Enables Linux performance counter support
- `perfetto`: Enables Perfetto tracing integration
- `ittapi`: Enables Intel VTune integration
- `panic`: Converts errors from eprintln! to panic!

### Testing Approach

Tests use `rusty-fork` to isolate tracing subscriber initialization between tests. The `make_spans()` function creates a standard span hierarchy for testing various layers.