// Copyright 2024-2025 Irreducible Inc.

use tracing::span;
use tracing_subscriber::registry::LookupSpan;

use crate::errors::err_msg;

/// Register storage of the given type with the span.
#[allow(unused)]
pub fn insert_to_span_storage<T, S>(
    id: &span::Id,
    ctx: tracing_subscriber::layer::Context<'_, S>,
    storage: T,
) where
    T: 'static + Send + Sync,
    S: tracing::Subscriber,
    for<'lookup> S: LookupSpan<'lookup>,
{
    let Some(span) = ctx.span(id) else {
        return err_msg!("failed to get span");
    };

    span.extensions_mut().insert(storage);
}

/// Perform operation with mutable span storage value.
#[allow(unused)]
pub fn with_span_storage_mut<T, S>(
    id: &span::Id,
    ctx: tracing_subscriber::layer::Context<'_, S>,
    f: impl FnOnce(&mut T),
) where
    T: 'static,
    S: tracing::Subscriber,
    for<'lookup> S: LookupSpan<'lookup>,
{
    let Some(span) = ctx.span(id) else {
        return err_msg!("failed to get span");
    };

    let mut extensions = span.extensions_mut();
    let Some(storage) = extensions.get_mut::<T>() else {
        return err_msg!("Failed to get storage");
    };

    f(storage)
}
