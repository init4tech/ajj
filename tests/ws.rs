mod common;
use common::test_router;

use ajj::pubsub::{Connect, ServerShutdown};
use futures_util::{SinkExt, StreamExt};
use std::net::{Ipv4Addr, SocketAddr};
use tokio_tungstenite::{
    tungstenite::{client::IntoClientRequest, Message},
    MaybeTlsStream, WebSocketStream,
};

const WS_SOCKET: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 3383);
const WS_SOCKET_STR: &str = "ws://127.0.0.1:3383";

async fn serve_ws() -> ServerShutdown {
    let router = test_router();
    WS_SOCKET.serve(router).await.unwrap()
}

struct WsClient {
    socket: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
}

impl WsClient {
    async fn send<S: serde::Serialize>(&mut self, msg: &S) {
        self.socket
            .send(Message::Text(serde_json::to_string(msg).unwrap().into()))
            .await
            .unwrap();
    }

    async fn recv(&mut self) -> serde_json::Value {
        match self.socket.next().await.unwrap().unwrap() {
            Message::Text(text) => serde_json::from_str(&text).unwrap(),
            _ => panic!("unexpected message type"),
        }
    }
}

async fn ws_client() -> WsClient {
    let request = WS_SOCKET_STR.into_client_request().unwrap();
    let (socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    WsClient { socket }
}

#[tokio::test]
async fn basic_ws() {
    let _server = serve_ws().await;
    let mut client = ws_client().await;

    client
        .send(&serde_json::json!({
            "id": 1,
            "method": "ping",
            "params": []
        }))
        .await;

    let next = client.recv().await;
    assert_eq!(
        next,
        serde_json::json!({"id": 1, "jsonrpc": "2.0", "result": "pong"})
    );
}
