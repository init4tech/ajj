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

impl crate::Connect for ListenerOptions<'_> {
    type Listener = Listener;

    type Error = io::Error;

    async fn make_listener(self) -> Result<Self::Listener, Self::Error> {
        self.create_tokio()
    }
}
