mod common;
use common::{test_router, TestClient};

use ajj::pubsub::{Connect, ServerShutdown};
use futures_util::{SinkExt, StreamExt};
use serial_test::serial;
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
    id: usize,
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
}

impl TestClient for WsClient {
    fn next_id(&mut self) -> usize {
        let id = self.id;
        self.id += 1;
        id
    }

    async fn send_raw<S: serde::Serialize>(&mut self, msg: &S) {
        self.send_inner(msg).await;
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
#[serial]
async fn test_ws() {
    let _server = serve_ws().await;
    let mut client = ws_client().await;
    common::basic_tests(&mut client).await;
    common::batch_tests(&mut client).await;
}
