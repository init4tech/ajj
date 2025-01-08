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

    /// Configure the instruction buffer size for each task spawned by the
    /// listener. This buffer will be allocated for EACH connection, and
    /// represents the backpressure limit for notifications and responses
    /// sent to the client.
    fn instruction_buffer(&self) -> usize {
        crate::shared::DEFAULT_INSTRUCTION_BUFFER_PER_TASK
    }

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
                    instruction_buffer_per_task: crate::shared::DEFAULT_INSTRUCTION_BUFFER_PER_TASK,
                },
            }
            .spawn();
            Ok(tx.into())
        }
    }
}

/// A [`Listener`] accepts incoming connections and produces [`JsonSink`] and
/// [`JsonReqStream`] objects.
///
/// Typically this is done by producing a combined object with a [`Stream`] and
/// a [`Sink`], then using [`StreamExt::split`], however this trait allows for
/// more complex implementations.
///
/// It is expected that the stream or sink may have adapter types like
/// [`ReadJsonStream`] or [`WsJsonStream`] that wrap the underlying transport
/// library objects.
///
/// [`WsJsonStream`]: crate::WsJsonStream
/// [`ReadJsonStream`]: alloy::transports::ipc::ReadJsonStream
/// [`Sink`]: futures_util::sink::Sink
/// [`StreamExt::split`]: futures_util::stream::StreamExt::split
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
