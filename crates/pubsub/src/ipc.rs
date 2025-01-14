use crate::{
    shared::{ListenerTask, WriteTask},
    RouteTask,
};
use alloy::{rpc::json_rpc::PartiallySerializedRequest, transports::ipc::ReadJsonStream};
use interprocess::local_socket::{
    tokio::{Listener, RecvHalf, SendHalf},
    traits::tokio::Stream as _,
};
use serde_json::value::RawValue;
use std::io;
use tokio::io::AsyncWriteExt;

pub type IpcListenerTask = ListenerTask<interprocess::local_socket::tokio::Listener>;

pub type IpcRouteTask = RouteTask<interprocess::local_socket::tokio::Listener>;

/// The IpcWriteTask is responsible for writing responses to IPC sockets.
pub type IpcWriteTask = WriteTask<SendHalf>;

impl crate::Listener for Listener {
    type RespSink = SendHalf;

    type ReqStream = ReadJsonStream<RecvHalf, PartiallySerializedRequest>;

    type Error = io::Error;

    async fn accept(
        &self,
    ) -> Result<(Self::RespSink, Self::ReqStream), Self::Error> {
        let conn = interprocess::local_socket::traits::tokio::Listener::accept(self).await?;

        let (recv, send) = conn.split();

        Ok((send, recv.into()))
    }
}

impl crate::JsonSink for SendHalf {
    type Error = std::io::Error;

    async fn send_json(
        &mut self,
        json: Box<RawValue>,
    ) -> Result<(), Self::Error> { self.write_all(json.get().as_bytes()).await }
}
