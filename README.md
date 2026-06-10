# tracing-profile

This library implements subscribers for the [`tracing`](https://docs.rs/tracing/latest/tracing/) crate that facilitate
profiling of one-shot program executions. That is, this is not intended for profiling of long-running programs.

`tracing-profile` depends on [`tracing-subscriber`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/) and
implements layers that measure, record, and display timing information in the span graph. The implemented
`tracing_subscriber::Layer` records the time a span took to execute, along with any user supplied metadata and
information necessary to construct a span graph from the resulting logs.

Note that if `#[tracing::instrument]` is used, the `skip_all` argument is recommended. Omitting this will result in all
the function arguments being included as fields.

## Usage

The library exposes several layers that output the information in different ways.

## Feature flags
 - `gen_filename` enables `TraceFilenameBuilder` for customizable trace filenames (automatically enabled with `perfetto`)
 - `perfetto` enables Perfetto trace generation with customizable filenames

### PrintTreeLayer

The `PrintTreeLayer` processes the profiling information in the running process and prints the timing information in a
human-readable format. The output structures the span graph as a tree.

```
$ cargo test -- --nocapture
root span [ 112.67µs | 100.00% ]
├── child span1 [ 2.63µs | 2.33% ] { field1 = value1 }
└── child span2 [ 64.29µs | 57.06% ] { field2 = value2 }
   ├── child span3 [ 1.88µs | 1.66% ] { field3 = value3 }
   └── child span4 [ 1.67µs | 1.48% ] { field4 = value4 }
```

### IttApiLayer

The `IttApiLayer` adds one `ittapi::Task` for each span at runtime allowing observing spans in Intel VTune profiler. See [ITT API documentation](https://www.intel.com/content/www/us/en/docs/vtune-profiler/user-guide/2023-1/instrumentation-and-tracing-technology-apis.html) for reference.

`ittapi` crate feature enables this functionality.

### Perfetto layer

The `Perfetto` layer
 - for each tracing span span creates a single Perfetto event with the same lifetime
 - for each event that has `counter` property set to true updates the corresponding Perfetto counter value using the `value` property of the event.

See the documentation to the `PerfettoLayer` struct for more details.

_Perfetto is supported only if the target OS is Linux._

### Example Test

```rust
fn make_spans() {
    let span = debug_span!("root span");
    let _scope1 = span.enter();

    // child spans 1 and 2 are siblings
    let span2 = debug_span!("child span1", field1 = "value1");
    let scope2 = span2.enter();
    drop(scope2);

    let span3 = debug_span!("child span2", field2 = "value2");
    let _scope3 = span3.enter();

    // child spans 3 and 4 are siblings
    let span = debug_span!("child span3", field3 = "value3");
    let scope = span.enter();
    drop(scope);

    let span = debug_span!("child span4", field4 = "value4");
    let scope = span.enter();
    drop(scope);
}

#[test]
fn all_layers() {
    tracing_subscriber::registry()
        .with(PrintTreeLayer::default())
        .init();
    make_spans();
}
```

### Configuration

Using `PrintTreeConfig` you can configure color and aggregation/hiding thresholds.

```rs
#[test]
fn all_layers() {
    tracing_subscriber::registry()
        .with(PrintTreeLayer::new(PrintTreeConfig {
            attention_above_percent: 25.0,
            relevant_above_percent: 2.5,
            hide_below_percent: 1.0,
            display_unaccounted: false
        }))
        .init();
    make_spans();
}
```

## Authors

`tracing-profile` is developed and maintained by [Irreducible](https://www.irreducible.com/).

## License

MIT License

Copyright (c) 2024-2025 Irreducible Inc.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
