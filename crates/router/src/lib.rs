//! JSON-RPC router inspired by axum's `Router`.
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
//!
//! [`axum`]: https://docs.rs/axum/latest/axum/index.html
//! [`axum::Router`]: https://docs.rs/axum/latest/axum/routing/struct.Router.html
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

#[macro_use]
pub(crate) mod macros;

#[cfg(feature = "axum")]
mod axum;

mod error;
pub use error::RegistrationError;

mod primitives;
pub use primitives::MethodId;

mod routes;
pub(crate) use routes::{BoxedIntoRoute, ErasedIntoRoute, Method};
pub use routes::{Handler, HandlerService, Route};

mod router;
pub use router::Router;

#[cfg(test)]
mod test {

    use crate::router::RouterInner;
    use alloy::rpc::json_rpc::{ErrorPayload, ResponsePayload};
    use serde_json::value::RawValue;
    use std::borrow::Cow;

    // more of an example really
    #[tokio::test]
    async fn example() {
        let router: RouterInner<()> = RouterInner::new()
            .route_service(
                "hello_world",
                tower::service_fn(|_: Box<RawValue>| async {
                    Ok(
                        ResponsePayload::<(), u8>::internal_error_with_message_and_obj(
                            Cow::Borrowed("Hello, world!"),
                            30u8,
                        )
                        .serialize_payload()
                        .unwrap(),
                    )
                }),
            )
            .route("foo", |a: Box<RawValue>, _state: u64| async move {
                Ok::<_, ()>(a.get().to_owned())
            })
            .with_state(&3u64);

        let res = router
            .call_with_state("hello_world".into(), Default::default(), ())
            .await
            .expect("infallible")
            .deserialize_error()
            .unwrap();

        assert!(matches!(
            res,
            ResponsePayload::Failure(ErrorPayload {
                code: -32603,
                message: Cow::Borrowed("Hello, world!"),
                data: Some(30u8)
            })
        ),);

        let res2 = dbg!(
            router
                .call_with_state(
                    "foo".into(),
                    RawValue::from_string("{}".to_owned()).unwrap(),
                    ()
                )
                .await
        )
        .expect("infallible")
        .deserialize_success::<String>()
        .unwrap()
        .deserialize_error::<()>()
        .unwrap();

        assert_eq!(res2, ResponsePayload::Success("{}".to_owned()));
    }
}

// Some code is this file is reproduced under the terms of the MIT license. It
// originates from the `axum` crate. The original source code can be found at
// the following URL, and the original license is included below.
//
// https://github.com/tokio-rs/axum/blob/f84105ae8b078109987b089c47febc3b544e6b80/axum/src/routing/mod.rs#L119
//
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
