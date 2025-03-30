#![cfg(all(feature = "pubsub", any(feature = "ws", feature = "axum")))]

use super::TestClient;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{
    tungstenite::{client::IntoClientRequest, Message},
    MaybeTlsStream, WebSocketStream,
};

/// Create a WebSocket client for testing.
#[allow(dead_code)]
pub async fn ws_client(s: &str) -> WsClient {
    let request = s.into_client_request().unwrap();
    let (socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    WsClient { socket, id: 0 }
}

pub struct WsClient {
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
