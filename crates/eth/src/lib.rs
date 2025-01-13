// use alloy::{
//     primitives::{Address, U256},
//     rpc::types::BlockId,
// };
// use axum::routing::post;
// use reth_rpc_eth_api::helpers::{EthState, FullEthApi};
// use std::{borrow::Borrow, sync::Arc};
// use tracing::{debug_span, Instrument};

// #[derive(Clone)]
// pub struct EthRpcContext<T> {
//     inner: Arc<T>,
// }

// impl<T> AsRef<T> for EthRpcContext<T> {
//     fn as_ref(&self) -> &T {
//         &self.inner
//     }
// }

// impl<T> Borrow<T> for EthRpcContext<T> {
//     fn borrow(&self) -> &T {
//         &self.inner
//     }
// }

// impl<T> core::ops::Deref for EthRpcContext<T> {
//     type Target = T;

//     fn deref(&self) -> &T {
//         &self.inner
//     }
// }

// impl<T> From<T> for EthRpcContext<T>
// where
//     T: FullEthApi,
// {
//     fn from(inner: T) -> Self {
//         Self {
//             inner: Arc::new(inner),
//         }
//     }
// }

// macro_rules! convert_route {
//     ($call:expr) => {
//         convert_route!($call,)
//     };

//     ($call:expr,) => {
//         convert_route!(finish @ $call.in_current_span())

//     };

//     ($call:expr, $name:literal) => {
//         convert_route!($call, $name,)
//     };

//     ($call:expr, $name:literal,) => {
//         let span = tracing::info_span!("rpc", method = $name);
//         convert_route!($call, span)
//     };

//     ($call:expr, $span:expr) => {
//         convert_route!($call,$span,)
//     };

//     ($call:expr, $span:expr,) => {
//         {
//             convert_route!(finish @ $call.instrument($span))
//         }
//     };

//     (finish @ $fut:expr) => {
//         {
//             $fut
//                 .await
//                 .inspect_err(|err| tracing::warn!(%err, "rpc error"))
//                 .map_err(drop)
//     }
//     };
// }

// /// [`Handler`] for the `eth_getTransactionCount` RPC method.
// ///
// /// [`Handler`]: router::Handler
// #[allow(non_snake_case)]
// pub async fn eth_getTransactionCount<T>(
//     params: (Address, Option<BlockId>),
//     state: EthRpcContext<T>,
// ) -> Result<U256, ()>
// where
//     T: FullEthApi,
// {
//     let span = debug_span!("rpc", method = "getTransactionCount", address = %params.0, block_id = ?params.1);

//     convert_route!(
//         EthState::transaction_count(state.as_ref(), params.0, params.1),
//         span
//     )
// }

// pub fn eth_router<T: FullEthApi>(t: T) -> axum::Router<()> {
//     let t = t.into();

//     let router = router::Router::<EthRpcContext<T>>::new()
//         .route("getTransactionCount", eth_getTransactionCount)
//         .route("getCode", |(address, block), state: EthRpcContext<T>| {
//             async move { convert_route!(EthState::get_code(&*state, address, block)) }
//                 .instrument(debug_span!("rpc", method = "getCode"))
//         })
//         .route("getBalance", |(address, block), state: EthRpcContext<T>| {
//             async move { convert_route!(EthState::balance(&*state, address, block)) }
//                 .instrument(debug_span!("rpc", method = "getBalance"))
//         })
//         .with_state(t);

//     let router = router::Router::new().nest("eth", router);

//     axum::Router::new().route("/rpc", post(router))
// }
