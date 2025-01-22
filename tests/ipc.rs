mod common;
use common::test_router;

use ajj::pubsub::{Connect, ReadJsonStream, ServerShutdown};
use futures_util::StreamExt;
use interprocess::local_socket::{
    self as ls,
    tokio::{prelude::LocalSocketStream, RecvHalf, SendHalf},
    traits::tokio::Stream,
    ListenerOptions,
};
use serde_json::Value;
use tempfile::{NamedTempFile, TempPath};
use tokio::io::AsyncWriteExt;

pub(crate) fn to_name(path: &std::ffi::OsStr) -> std::io::Result<ls::Name<'_>> {
    if cfg!(windows) && !path.as_encoded_bytes().starts_with(br"\\.\pipe\") {
        ls::ToNsName::to_ns_name::<ls::GenericNamespaced>(path)
    } else {
        ls::ToFsName::to_fs_name::<ls::GenericFilePath>(path)
    }
}

async fn serve_ipc() -> (ServerShutdown, TempPath) {
    let router = test_router();

    let temp = NamedTempFile::new().unwrap().into_temp_path();
    let name = to_name(temp.as_os_str()).unwrap();

    dbg!(&name);
    dbg!(std::fs::remove_file(&temp).unwrap());

    let shutdown = ListenerOptions::new()
        .name(name)
        .serve(router)
        .await
        .unwrap();
    (shutdown, temp)
}

struct IpcClient {
    recv_half: ReadJsonStream<RecvHalf, Value>,
    send_half: SendHalf,
}

impl IpcClient {
    async fn new(temp: &TempPath) -> Self {
        let name = to_name(temp.as_os_str()).unwrap();
        let (recv_half, send_half) = LocalSocketStream::connect(name).await.unwrap().split();
        Self {
            recv_half: recv_half.into(),
            send_half,
        }
    }

    async fn send<S: serde::Serialize>(&mut self, msg: &S) {
        let s = serde_json::to_string(msg).unwrap();

        self.send_half.write_all(s.as_bytes()).await.unwrap();
    }

    async fn recv(&mut self) -> serde_json::Value {
        self.recv_half.next().await.unwrap()
    }
}

#[tokio::test]
async fn basic_ipc() {
    let (_server, temp) = serve_ipc().await;
    let mut client = IpcClient::new(&temp).await;

    client
        .send(&serde_json::json!({
            "id": 1,
            "method": "ping",
            "params": []
        }))
        .await;

    tracing::info!("sent");
    let next = client.recv().await;
    tracing::info!("received");
    assert_eq!(
        next,
        serde_json::json!({"id": 1, "jsonrpc": "2.0", "result": "pong"})
    );
}
