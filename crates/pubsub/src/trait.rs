use crate::{
    shared::{ConnectionManager, ListenerTask},
    ServerShutdown,
};
use alloy::rpc::json_rpc::PartiallySerializedRequest;
use serde_json::value::RawValue;
use std::future::Future;
use tokio::sync::watch;
use tokio_stream::Stream;

/// Convenience alias for naming stream halves.
pub type Out<T> = <T as Listener>::RespSink;

/// Convenience alias for naming stream halves.
pub type In<T> = <T as Listener>::ReqStream;

/// Configuration objects for connecting a [`Listener`].
pub trait Connect: Send + Sync + Sized {
    type Listener: Listener;
    type Error: core::error::Error + 'static;

    /// Create the listener
    fn make_listener(self) -> impl Future<Output = Result<Self::Listener, Self::Error>> + Send;

    fn run(
        self,
        router: router::Router<()>,
    ) -> impl Future<Output = Result<ServerShutdown, Self::Error>> + Send {
        async move {
            let (tx, rx) = watch::channel(());
            ListenerTask {
                listener: self.make_listener().await?,
                manager: ConnectionManager {
                    shutdown: rx,
                    next_id: 0,
                    router,
                },
            }
            .spawn();
            Ok(tx.into())
        }
    }
}

/// A [`Listener`] accepts incoming connections and produces ``
pub trait Listener: Send + 'static {
    type RespSink: JsonSink;
    type ReqStream: JsonReqStream;
    type Error: core::error::Error;

    fn accept(
        &self,
    ) -> impl Future<Output = Result<(Self::RespSink, Self::ReqStream), Self::Error>> + Send;
}

pub trait JsonSink: Send + 'static {
    type Error: core::error::Error + 'static;

    fn send_json(
        &mut self,
        json: Box<RawValue>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

impl JsonSink for tokio::sync::mpsc::Sender<Box<RawValue>> {
    type Error = tokio::sync::mpsc::error::SendError<Box<RawValue>>;

    fn send_json(
        &mut self,
        json: Box<RawValue>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.send(json)
    }
}

/// A stream of inbound [`PartiallySerializedRequest`] objects.
pub trait JsonReqStream:
    Stream<Item = PartiallySerializedRequest> + Send + Unpin + 'static
{
}

impl<T> JsonReqStream for T where
    T: Stream<Item = PartiallySerializedRequest> + Send + Unpin + 'static
{
}
