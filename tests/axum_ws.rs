#![cfg(all(feature = "ws", feature = "axum"))]

mod common;
use common::{test_router, ws_client::ws_client};

use ajj::pubsub::AxumWsCfg;
use axum::routing::any;

const SOCKET_STR: &str = "127.0.0.1:3399";
const URL: &str = "ws://127.0.0.1:3399";

/// Serve the WebSocket server using Axum.
async fn serve() {
    let router = test_router();

    let axum_router = axum::Router::new()
        .route("/", any(ajj::pubsub::ajj_websocket))
        .with_state::<()>(AxumWsCfg::new(router));

    let listener = tokio::net::TcpListener::bind(SOCKET_STR).await.unwrap();
    axum::serve(listener, axum_router).await.unwrap();
}

#[tokio::test]
async fn test_ws() {
    let _server = tokio::spawn(serve());

    // Give the server a moment to start
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    let mut client = ws_client(URL).await;
    common::basic_tests(&mut client).await;
    common::batch_tests(&mut client).await;
}
