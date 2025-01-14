use alloy::{rpc::json_rpc::PartiallySerializedRequest, transports::ipc::ReadJsonStream};
use interprocess::local_socket::{
    tokio::{Listener, RecvHalf, SendHalf},
    traits::tokio::Stream as _,
    ListenerOptions,
};
use serde_json::value::RawValue;
use std::io;
use tokio::io::AsyncWriteExt;

impl crate::Listener for Listener {
    type RespSink = SendHalf;

    type ReqStream = ReadJsonStream<RecvHalf, PartiallySerializedRequest>;

    type Error = io::Error;

    async fn accept(&self) -> Result<(Self::RespSink, Self::ReqStream), Self::Error> {
        let conn = interprocess::local_socket::traits::tokio::Listener::accept(self).await?;

        let (recv, send) = conn.split();

        Ok((send, recv.into()))
    }
}

impl crate::JsonSink for SendHalf {
    type Error = std::io::Error;

    async fn send_json(&mut self, json: Box<RawValue>) -> Result<(), Self::Error> {
        self.write_all(json.get().as_bytes()).await
    }
}

impl<'a> crate::Connect for ListenerOptions<'a> {
    type Listener = Listener;

    type Error = io::Error;

    fn make_listener(
        self,
    ) -> impl std::future::Future<Output = Result<Self::Listener, Self::Error>> + Send {
        async move { self.create_tokio() }
    }
}
