//! ajj: A JSON-RPC router inspired by axum's `Router`.
//!
//! This crate provides a way to define a JSON-RPC router that can be used to
//! route requests to handlers. It is inspired by the [`axum`] crate's
//! [`axum::Router`].
//!
//! ## Basic usage
//!
//! The [`Router`] type is the main type provided by this crate. It is used to
//! register JSON-RPC methods and their handlers.
//!
//! ```no_run
//! use ajj::{Router, HandlerCtx, ResponsePayload};
//!
//! # fn test_fn() -> Router<()> {
//! // Provide methods called "double" and "add" to the router.
//! let router = Router::<u64>::new()
//!   .route("add", |params: u64, state: u64| async move {
//!     Ok::<_, ()>(params + state)
//!   })
//!   .with_state(3u64)
//!   .route("double", |params: u64| async move {
//!     Ok::<_, ()>(params * 2)
//!   })
//!   // Routes get a ctx, which can be used to send notifications.
//!   .route("notify", |ctx: HandlerCtx| async move {
//!     if ctx.notifications().is_none() {
//!       // This error will appear in the ResponsePayload's `data` field.
//!       return Err("notifications are disabled");
//!     }
//!
//!     let req_id = 15u8;
//!
//!     tokio::task::spawn_blocking(move || {
//!       // something expensive goes here
//!       let result = 100_000_000;
//!       let _ = ctx.notify(&serde_json::json!({
//!         "req_id": req_id,
//!         "result": result,
//!       }));
//!     });
//!     Ok(req_id)
//!   })
//!   .route("error_example", || async {
//!     // This will appear in the ResponsePayload's `message` field.
//!     ResponsePayload::<(), ()>::internal_error_message("this is an error".into())
//!   });
//! # router
//! # }
//! ```
//!
//! ## Handlers
//!
//! Methods are routed via the [`Handler`] trait, which is blanket implemented
//! for many async functions. [`Handler`] contain implement the logic executed
//! when calling methods on the JSON-RPC router.
//!
//! Handlers can return either
//! - `Result<T, E> where T: Serialize, E: Serialize`
//! - `ResponsePayload<T, E> where T: Serialize, E: Serialize`
//!
//! These types will be serialized into the JSON-RPC response. The `T` type
//! represents the result of the method, and the `E` type represents an error
//! response. The `E` type is optional, and can be set to `()` if no error
//! response is needed.
//!
//! See the [`Handler`] trait docs for more information.
//!
//! ## Serving the Router
//!
//! We recommend [`axum`] for serving the router over HTTP. When the `"axum"`
//! feature flag is enabled, The [`Router`] provides
//! `Router::into_axum(path: &str)` to instantiate a new [`axum::Router`], and
//! register the router to handle requests. You can then serve the
//! [`axum::Router`] as normal, or add additional routes to it.
//!
//! ```no_run
//! # #[cfg(feature = "axum")]
//! # {
//! # use ajj::{Router, HandlerCtx, ResponsePayload};
//! # async fn _main(router: Router<()>) {
//! // Instantiate a new axum router, and register the JSON-RPC router to handle
//! // requests at the `/rpc` path, and serve it on port 3000.
//! let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
//! axum::serve(listener, router.into_axum("/rpc")).await.unwrap();
//! # }}
//! ```
//!
//! For WS and IPC connections, the `pubsub` module provides implementations of
//! the `Connect` trait for [`std::net::SocketAddr`] to create simple WS
//! servers, and [`interprocess::local_socket::ListenerOptions`] to create
//! simple IPC servers.
//!
//! ```no_run
//! # #[cfg(feature = "pubsub")]
//! # {
//! # use ajj::{Router, pubsub::Connect};
//! # async fn _main(router:Router<()>) {
//! // Serve the router over websockets on port 3000.
//! let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 3000));
//! // The shutdown object will stop the server when dropped.
//! let shutdown = addr.serve(router).await.unwrap();
//! # }}
//! ```
//!
#![cfg_attr(
    feature = "pubsub",
    doc = "See the [`pubsub`] module documentation for more information."
)]
//!
//! [`axum`]: https://docs.rs/axum/latest/axum/index.html
//! [`axum::Router`]: https://docs.rs/axum/latest/axum/routing/struct.Router.html
//! [`ResponsePayload`]: alloy::rpc::json_rpc::ResponsePayload
//! [`interprocess::local_socket::ListenerOptions`]: https://docs.rs/interprocess/latest/interprocess/local_socket/struct.ListenerOptions.html

#![warn(
    missing_copy_implementations,
    missing_debug_implementations,
    missing_docs,
    unreachable_pub,
    clippy::missing_const_for_fn,
    rustdoc::all
)]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![deny(unused_must_use, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

#[macro_use]
pub(crate) mod macros;

#[cfg(feature = "axum")]
mod axum;

mod error;
pub use error::RegistrationError;

mod primitives;
pub use primitives::{BorrowedRpcObject, MethodId, RpcBorrow, RpcObject, RpcRecv, RpcSend};

#[cfg(feature = "pubsub")]
pub mod pubsub;
#[doc(hidden)] // for tests
#[cfg(feature = "ipc")]
pub use pubsub::ReadJsonStream;

mod routes;
pub use routes::{
    BatchFuture, Handler, HandlerArgs, HandlerCtx, NotifyError, Params, RouteFuture, State,
};
pub(crate) use routes::{BoxedIntoRoute, ErasedIntoRoute, Method, Route};

mod router;
pub use router::Router;

mod tasks;
pub(crate) use tasks::TaskSet;

mod types;
pub use types::{ErrorPayload, ResponsePayload};

/// Re-export of the `tower` crate, primarily to provide [`tower::Service`],
/// and [`tower::service_fn`].
pub use tower;

/// Re-export of the `serde_json` crate, primarily to provide the `RawValue` type.
pub use serde_json::{self, value::RawValue};

#[cfg(test)]
pub(crate) mod test_utils {
    use serde_json::value::RawValue;

    #[track_caller]
    pub(crate) fn assert_rv_eq(a: &RawValue, b: &str) {
        let left = serde_json::from_str::<serde_json::Value>(a.get()).unwrap();
        let right = serde_json::from_str::<serde_json::Value>(b).unwrap();
        assert_eq!(left, right);
    }
}

#[cfg(test)]
mod test {

    use crate::{
        router::RouterInner, routes::HandlerArgs, test_utils::assert_rv_eq, ResponsePayload,
    };
    use bytes::Bytes;
    use serde_json::value::RawValue;
    use std::borrow::Cow;

    // more of an example really
    #[tokio::test]
    async fn example() {
        let router: RouterInner<()> = RouterInner::new()
            .route("hello_world", || async {
                ResponsePayload::<(), _>::internal_error_with_message_and_obj(
                    Cow::Borrowed("Hello, world!"),
                    30u8,
                )
            })
            .route("foo", |a: Box<RawValue>, _state: u64| async move {
                Ok::<_, ()>(a.get().to_owned())
            })
            .with_state(&3u64);

        let req = Bytes::from_static(r#"{"id":1,"method":"hello_world","params":[]}"#.as_bytes());

        let res = router
            .call_with_state(
                HandlerArgs {
                    ctx: Default::default(),
                    req: req.try_into().unwrap(),
                },
                (),
            )
            .await
            .expect("infallible")
            .expect("request had ID, is not a notification");

        assert_rv_eq(
            &res,
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32603,"message":"Hello, world!","data":30}}"#,
        );

        let req2 = Bytes::from_static(r#"{"id":1,"method":"foo","params":{}}"#.as_bytes());

        let res2 = router
            .call_with_state(
                HandlerArgs {
                    ctx: Default::default(),
                    req: req2.try_into().unwrap(),
                },
                (),
            )
            .await
            .expect("infallible")
            .expect("request had ID, is not a notification");

        assert_rv_eq(&res2, r#"{"jsonrpc":"2.0","id":1,"result":"{}"}"#);
    }
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
