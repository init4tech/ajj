/// Used by the [`Router`] to modify the type of the [`RouterInner`] and return
/// a new [`Router`].
///
/// [`Router`]: crate::Router
/// [`RouterInner`]: crate::router::RouterInner
macro_rules! map_inner {
    ( $self_:ident, $inner:pat_param => $expr:expr) => {
        #[allow(redundant_semicolons)]
        {
            let $inner = $self_.into_inner();
            Router {
                inner: Arc::new($expr),
            }
        }
    };
}

/// Used by the [`Router`] to access methods on the [`RouterInner`] without
/// modifying the inner type.
///
/// [`Router`]: crate::Router
/// [`RouterInner`]: crate::router::RouterInner
macro_rules! tap_inner {
    ( $self_:ident, mut $inner:ident => { $($stmt:stmt)* } ) => {
        #[allow(redundant_semicolons)]
        {
            let mut $inner = $self_.into_inner();
            $($stmt)*
            Router {
                inner: Arc::new($inner),
            }
        }
    };
}

/// Unwrap a result, panic with the `Display` of the error if it is an `Err`.
macro_rules! panic_on_err {
    ($expr:expr) => {
        match $expr {
            Ok(x) => x,
            Err(err) => panic!("{err}"),
        }
    };
}

/// Unwrap a result contianing an `Infallible`.
#[allow(unused_macros)] // used in some features
macro_rules! unwrap_infallible {
    ($expr:expr) => {
        match $expr {
            Ok(x) => x,
            Err(_) => unreachable!("Infallible"),
        }
    };
}

/// Log a message event to the current span.
///
/// See <https://github.com/open-telemetry/semantic-conventions/blob/d66109ff41e75f49587114e5bff9d101b87f40bd/docs/rpc/rpc-spans.md#events>
macro_rules! message_event {
    ($type:literal, counter: $counter:expr, bytes: $bytes:expr,) => {{
        ::tracing::info!(
            "rpc.message.id" = $counter.fetch_add(1, ::std::sync::atomic::Ordering::Relaxed),
            "rpc.message.type" = $type,
            "rpc.message.uncompressed_size" = $bytes,
            "rpc.message"
        );
    }};

    (@received, counter: $counter:expr, bytes: $bytes:expr, ) => {
        message_event!("RECEIVED", counter: $counter, bytes: $bytes,);
    };

    (@sent, counter: $counter:expr, bytes: $bytes:expr, ) => {
        message_event!("SENT", counter: $counter, bytes: $bytes,);
    };
}

// Some code is this file is reproduced under the terms of the MIT license. It
// originates from the `axum` crate. The original source code can be found at
// the following URL, and the original license is included below.
//
// https://github.com/tokio-rs/axum/blob/f84105ae8b078109987b089c47febc3b544e6b80/axum/src/routing/mod.rs#L119
//
// The MIT License (MIT)
//
// Copyright (c) 2019 Axum Contributors
//
// Permission is hereby granted, free of charge, to any
// person obtaining a copy of this software and associated
// documentation files (the "Software"), to deal in the
// Software without restriction, including without
// limitation the rights to use, copy, modify, merge,
// publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software
// is furnished to do so, subject to the following
// conditions:
//
// The above copyright notice and this permission notice
// shall be included in all copies or substantial portions
// of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
// ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
// TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
// PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
// SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
// IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.
