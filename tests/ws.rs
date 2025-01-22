mod common;
use common::{test_router, TestClient};

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
    id: u64,
}

impl WsClient {
    async fn send_inner<S: serde::Serialize>(&mut self, msg: &S) {
        self.socket
            .send(Message::Text(serde_json::to_string(msg).unwrap().into()))
            .await
            .unwrap();
    }

    async fn recv_inner<D: serde::de::DeserializeOwned>(&mut self) -> D {
        match self.socket.next().await.unwrap().unwrap() {
            Message::Text(text) => serde_json::from_str(&text).unwrap(),
            _ => panic!("unexpected message type"),
        }
    }

    fn next_id(&mut self) -> u64 {
        let id = self.id;
        self.id += 1;
        id
    }
}

impl TestClient for WsClient {
    async fn send<S: serde::Serialize>(&mut self, method: &str, params: &S) {
        let id = self.next_id();
        self.send_inner(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))
        .await;
    }

    async fn recv<D: serde::de::DeserializeOwned>(&mut self) -> D {
        self.recv_inner().await
    }
}

async fn ws_client() -> WsClient {
    let request = WS_SOCKET_STR.into_client_request().unwrap();
    let (socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    WsClient { socket, id: 0 }
}

#[tokio::test]
async fn basic_ws() {
    let _server = serve_ws().await;
    let client = ws_client().await;
    common::basic_tests(client).await;
}
