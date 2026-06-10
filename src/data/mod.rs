// Copyright 2024-2025 Irreducible Inc.

mod event_counts;
mod field_visitor;
mod guard_wrapper;
mod log_tree;
mod span_metadata;
mod storage_utils;

pub(crate) use event_counts::EventCounts;
#[allow(unused_imports)]
pub use field_visitor::{CounterValue, CounterVisitor, StoringFieldVisitor, WritingFieldVisitor};
#[allow(unused_imports)]
pub(super) use guard_wrapper::GuardWrapper;
pub use log_tree::LogTree;
#[cfg(feature = "perfetto")]
pub use span_metadata::*;
#[cfg(feature = "ittapi")]
pub use storage_utils::insert_to_span_storage;
#[cfg(any(feature = "perfetto", feature = "ittapi"))]
pub use storage_utils::with_span_storage_mut;
