use std::thread;
use std::time::Duration;

use tracing::{debug_span, event, instrument::WithSubscriber, level_filters::LevelFilter, Level};
use tracing_profile::{PrintTreeConfig, PrintTreeLayer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

#[tracing::instrument]
fn sleep_a_bit() {
    thread::sleep(Duration::from_millis(10));
}

#[tracing::instrument]
fn sleep_a_bit_and_yell() {
    thread::sleep(Duration::from_millis(10));
    event!(name: "yelling", Level::DEBUG, { value = 5 });
}

#[tracing::instrument]
fn sleep_a_bit_with_args(foo: &str, bar: u32) {
    thread::sleep(Duration::from_millis(10));
    sleep_a_bit();
}

#[tracing::instrument(skip(foo, bar))]
fn sleep_a_bit_with_skipped(foo: &str, bar: u32) {
    thread::sleep(Duration::from_millis(10));
    sleep_a_bit();
}

#[tracing::instrument]
fn root_fn() {
    sleep_a_bit();
    sleep_a_bit();
    sleep_a_bit();
    sleep_a_bit_and_yell();
    sleep_a_bit_with_args("hello", 42);
    sleep_in_thread();

    for u in 0..5 {
        sleep_a_bit_with_args("in loop", u);
    }
    for u in 0..5 {
        sleep_a_bit_with_skipped("in loop", u);
    }
}

fn sleep_in_thread() {
    thread::spawn(|| {
        sleep_a_bit_with_args(&"Im in a thread", 22);
    })
    .join()
    .unwrap();
}

fn make_spans() {
    event!(name: "event outside of span", Level::DEBUG, {value = 10});
    event!(name: "test_instant_event", Level::DEBUG, test_key = "test_value");

    {
        root_fn();
    }
    event!(name: "event after last span", Level::DEBUG, {value = 20});
}

fn main() {
    let (tree_layer, _guard) = PrintTreeLayer::new(PrintTreeConfig {
        attention_above_percent: 25.0,
        relevant_above_percent: 2.5,
        hide_below_percent: 1.0,
        display_unaccounted: false,
        no_color: true,
        accumulate_spans_count: true,
        accumulate_events: true,
        aggregate_similar_siblings: true,
    });
    let perf_filter = EnvFilter::builder()
        .with_default_directive("trace".parse().unwrap())
        .with_env_var("RUST_PERF_LOG")
        .from_env_lossy();

    tracing_subscriber::registry()
        .with(tree_layer.with_filter(perf_filter))
        .init();

    make_spans();
}
