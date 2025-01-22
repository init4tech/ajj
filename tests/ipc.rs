mod common;
use common::{test_router, TestClient};

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
use tracing::Level;

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
    id: usize,
}

impl IpcClient {
    async fn new(temp: &TempPath) -> Self {
        let name = to_name(temp.as_os_str()).unwrap();
        let (recv_half, send_half) = LocalSocketStream::connect(name).await.unwrap().split();
        Self {
            recv_half: recv_half.into(),
            send_half,
            id: 0,
        }
    }

    async fn send_inner<S: serde::Serialize>(&mut self, msg: &S) {
        let s = serde_json::to_string(msg).unwrap();

        self.send_half.write_all(s.as_bytes()).await.unwrap();
    }

    async fn recv_inner(&mut self) -> serde_json::Value {
        self.recv_half.next().await.unwrap()
    }
}

impl TestClient for IpcClient {
    fn next_id(&mut self) -> usize {
        let id = self.id;
        self.id += 1;
        id
    }

    fn last_id(&self) -> usize {
        self.id - 1
    }

    async fn send_raw<S: serde::Serialize>(&mut self, msg: &S) {
        self.send_inner(msg).await;
    }

    async fn recv<D: serde::de::DeserializeOwned>(&mut self) -> D {
        serde_json::from_value(self.recv_inner().await).unwrap()
    }
}

#[tokio::test]
async fn basic_ipc() {
    let (_server, temp) = serve_ipc().await;
    let client = IpcClient::new(&temp).await;

    common::basic_tests(client).await;
}
