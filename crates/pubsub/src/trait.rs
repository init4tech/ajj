use alloy::rpc::json_rpc::PartiallySerializedRequest;
use serde_json::value::RawValue;
use std::future::Future;
use tokio_stream::Stream;

/// Convenience alias for naming stream halves.
pub type Out<T> = <T as Listener>::RespSink;

/// Convenience alias for naming stream halves.
pub type In<T> = <T as Listener>::ReqStream;

/// Configuration objects for connecting a [`Listener`].
pub trait Connect: Clone + Send + Sync + 'static {
    type Listener: Listener;

    fn connect(self) -> impl Future<Output = Self::Listener> + Send;
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
