use crate::{
    shared::{InstructionBody, ListenerTask, Manager, WriteTask},
    RouteTask,
};
use alloy::{rpc::json_rpc::PartiallySerializedRequest, transports::ipc::ReadJsonStream};
use interprocess::local_socket::{
    tokio::{Listener, RecvHalf, SendHalf},
    traits::tokio::{Listener as _, Stream as _},
};
use serde_json::value::RawValue;
use std::{future::Future, io};
use tokio::{io::AsyncWriteExt, select, task::JoinHandle};

pub type IpcListenerTask = ListenerTask<interprocess::local_socket::tokio::Listener>;

pub type IpcRouteTask = RouteTask<interprocess::local_socket::tokio::Listener>;

/// The IpcWriteTask is responsible for writing responses to IPC sockets.
pub type IpcWriteTask = WriteTask<SendHalf>;

impl crate::Listener for Listener {
    type RespSink = SendHalf;

    type ReqStream = ReadJsonStream<RecvHalf, PartiallySerializedRequest>;

    type Error = io::Error;

    fn accept(
        &self,
    ) -> impl Future<Output = Result<(Self::RespSink, Self::ReqStream), Self::Error>> + Send {
        async {
            let conn = interprocess::local_socket::traits::tokio::Listener::accept(self).await?;

            let (recv, send) = conn.split();

            Ok((send, recv.into()))
        }
    }
}

impl crate::JsonSink for SendHalf {
    type Error = std::io::Error;

    fn send_json(
        &mut self,
        json: Box<RawValue>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        async move { self.write_all(json.get().as_bytes()).await }
    }
}
