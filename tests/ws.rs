#![cfg(feature = "ws")]

mod common;
use common::{test_router, ws_client::ws_client};

use ajj::pubsub::{Connect, ServerShutdown};
use std::net::{Ipv4Addr, SocketAddr};

const WS_SOCKET: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 3383);
const WS_SOCKET_STR: &str = "ws://127.0.0.1:3383";

async fn serve_ws() -> ServerShutdown {
    let router = test_router();
    WS_SOCKET.serve(router).await.unwrap()
}

#[tokio::test]
async fn test_ws() {
    let _server = serve_ws().await;
    let mut client = ws_client(WS_SOCKET_STR).await;
    common::basic_tests(&mut client).await;
    common::batch_tests(&mut client).await;
}
