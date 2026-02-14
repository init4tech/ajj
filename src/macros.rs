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

/// Implement a `Handler` call, with metrics recording and response building.
macro_rules! impl_handler_call {
    (@metrics, $success:expr, $id:expr, $service:expr, $method:expr) => {{
        // Record the metrics.
        $crate::metrics::record_execution($success, $service, $method);
        $crate::metrics::record_output($id.is_some(), $service, $method);
    }};

    (@record_span, $span:expr, $payload:expr) => {
        if let Some(e) = $payload.as_error() {
            use tracing_opentelemetry::OpenTelemetrySpanExt;
            $span.record("rpc.jsonrpc.error_code", e.code);
            $span.record("rpc.jsonrpc.error_message", e.message.as_ref());
            $span.set_status(::opentelemetry::trace::Status::Error {
                description: e.message.clone(),
            });
        }
    };

    // Hit the metrics and return the payload if any.
    (@finish $span:expr, $id:expr, $service:expr, $method:expr, $payload:expr) => {{
        impl_handler_call!(@metrics, $payload.is_success(), $id, $service, $method);
        impl_handler_call!(@record_span, $span, $payload);
        return Response::build_response($id.as_deref(), $payload);
    }};

    (@unpack_params $span:expr, $id:expr, $service:expr, $method:expr, $req:expr) => {{
            let Ok(params) = $req.deser_params() else {
            impl_handler_call!(@finish $span, $id, $service, $method, ResponsePayload::<(), ()>::invalid_params());
        };
        drop($req); // no longer needed
        params
    }};

    (@unpack_struct_params $span:expr, $id:expr, $service:expr, $method:expr, $req:expr) => {{
            let Ok(params) = $req.deser_params() else {
            impl_handler_call!(@finish $span, $id, $service, $method, ResponsePayload::<(), ()>::invalid_params());
        };
        drop($req); // no longer needed
        params
    }};

    (@unpack $args:expr) => {{
        let id = $args.id_owned();
        let (ctx, req) = $args.into_parts();
        let inst = ctx.span().clone();
        let span = ctx.span().clone();
        let method = req.method().to_string();
        let service = ctx.service_name();

        (id, ctx, inst, span, method, service, req)
    }};

    // NO ARGS
    ($args:expr, $this:ident()) => {{
        let (id, ctx, inst, span, method, service, req) = impl_handler_call!(@unpack $args);
        drop(ctx); // no longer needed
        drop(req); // no longer needed

        Box::pin(
            async move {
                let payload: $crate::ResponsePayload<_, _> = $this().await.into();
                impl_handler_call!(@finish span, id, service, &method, payload);
            }
            .instrument(inst),
        )
    }};

    // CTX only
    ($args:expr, $this:ident(ctx)) => {{
        let (id, ctx, inst, span, method, service, req) = impl_handler_call!(@unpack $args);
        drop(req); // no longer needed

        Box::pin(
            async move {
                let payload: $crate::ResponsePayload<_, _> = $this(ctx).await.into();
                impl_handler_call!(@finish span, id, service, &method, payload);
            }
            .instrument(inst),
        )
    }};


    // PARAMS only
    ($args:expr, $this:ident(params: $params_ty:ty)) => {{
        let (id, ctx, inst, span, method, service, req) = impl_handler_call!(@unpack $args);
        drop(ctx); // no longer needed

        Box::pin(
            async move {
                let params: $params_ty = impl_handler_call!(@unpack_params span, id, service, &method, req);
                let payload: $crate::ResponsePayload<_, _> = $this(params.into()).await.into();
                impl_handler_call!(@finish span, id, service, &method, payload);
            }
            .instrument(inst),
        )
    }};


    // STATE only
    ($args:expr, $this:ident($state:expr)) => {{
        let (id, ctx, inst, span, method, service, req) = impl_handler_call!(@unpack $args);
        drop(ctx); // no longer needed
        drop(req); // no longer needed

        Box::pin(
            async move {
                let payload: $crate::ResponsePayload<_, _> = $this($state).await.into();
                impl_handler_call!(@finish span, id, service, &method, payload);
            }
            .instrument(inst),
        )
    }};


    // CTX and PARAMS
    ($args:expr, $this:ident(ctx, params: $params_ty:ty)) => {{
        let (id, ctx, inst, span, method, service, req) = impl_handler_call!(@unpack $args);

        Box::pin(
            async move {
                let params: $params_ty = impl_handler_call!(@unpack_params span, id, service, &method, req);
                let payload: $crate::ResponsePayload<_, _> = $this(ctx, params.into()).await.into();
                impl_handler_call!(@finish span, id, service, &method, payload);
            }
            .instrument(inst),
        )
    }};

    // CTX and STATE
    ($args:expr, $this:ident(ctx, $state:expr)) => {{
        let (id, ctx, inst, span, method, service, req) = impl_handler_call!(@unpack $args);
        drop(req); // no longer needed

        Box::pin(
            async move {
                let payload: $crate::ResponsePayload<_, _> = $this(ctx, $state).await.into();
                impl_handler_call!(@finish span, id, service, &method, payload);
            }
            .instrument(inst),
        )
    }};

    // PARAMS and STATE
    ($args:expr, $this:ident(params: $params_ty:ty, $state:expr)) => {{
        let (id, ctx, inst, span, method, service, req) = impl_handler_call!(@unpack $args);
        drop(ctx); // no longer needed

        Box::pin(
            async move {
                let params: $params_ty = impl_handler_call!(@unpack_params span, id, service, &method, req);
                let payload: $crate::ResponsePayload<_, _> = $this(params.into(), $state).await.into();
                impl_handler_call!(@finish span, id, service, &method, payload);
            }
            .instrument(inst),
        )
    }};

    // CTX and PARAMS and STATE
    ($args:expr, $this:ident(ctx, params: $params_ty:ty, $state:expr)) => {{
        let (id, ctx, inst, span, method, service, req) = impl_handler_call!(@unpack $args);

        Box::pin(
            async move {
                let params: $params_ty = impl_handler_call!(@unpack_params span, id, service, &method, req);
                let payload: $crate::ResponsePayload<_, _> = $this(ctx, params.into(), $state).await.into();
                impl_handler_call!(@finish span, id, service, &method, payload);
            }
            .instrument(inst),
        )
    }};
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
