use crate::{
    pubsub::{
        shared::{ConnectionManager, ListenerTask, DEFAULT_NOTIFICATION_BUFFER_PER_CLIENT},
        ServerShutdown,
    },
    TaskSet,
};
use bytes::Bytes;
use serde_json::value::RawValue;
use std::future::Future;
use tokio::runtime::Handle;
use tokio_stream::Stream;

/// Convenience alias for naming stream halves.
pub type Out<T> = <T as Listener>::RespSink;

/// Convenience alias for naming stream halves.
pub type In<T> = <T as Listener>::ReqStream;

/// Configuration objects for connecting a [`Listener`].
///
/// This object is intended to capture all connection-related configuration and
/// setup, and output only the configured [`Listener`]. This allows it to
/// configure (e.g.) authentication or other connection-oriented policies,
/// without leaking logic into the router abstraction.
///
/// ## Implementer's guide
///
/// Implementing this trait requires either using an existing [`Listener`]
/// or implementing the [`Listener`], [`JsonSink`], and [`JsonReqStream`]
/// manually. Generally, we recommend re-using the existing [`Listener`]
/// implementations found in the `ws` and `ipc` modules.
///
/// When implementing this trait, careful consideration should be given the the
/// value returned by [`Connect::notification_buffer_size`]. This controls the
/// amount of server memory that a client can consume. Because JSON-RPC servers
/// may produce arbitrary response bodies, only the server developer can
/// accurately set this value. We have provided a low default. Setting it too
/// high may allow resource exhaustion attacks.
pub trait Connect: Send + Sync + Sized {
    /// The listener type produced by the connect object.
    type Listener: Listener;

    /// The error type for instantiating a [`Listener`].
    type Error: core::error::Error + 'static;

    /// Create the listener
    fn make_listener(self) -> impl Future<Output = Result<Self::Listener, Self::Error>> + Send;

    /// Configure the notification buffer size for each task spawned by the
    /// listener.
    ///
    /// This buffer will be allocated for EACH connection, and represents the
    /// backpressure limit for notifications and responses sent to the client.
    /// When the client is not reading from the connection, the buffer will
    /// fill. The server will then stop processing requests from that client
    /// until the buffer has room.
    ///
    /// The buffer contains `Box<RawValue>`, and the default size is
    /// [`DEFAULT_NOTIFICATION_BUFFER_PER_CLIENT`]. Because the buffer owns
    /// arbitrary JSON values, it is recommended to set this value based on
    /// the expected size of notifications and responses in your RPC server.
    fn notification_buffer_size(&self) -> usize {
        DEFAULT_NOTIFICATION_BUFFER_PER_CLIENT
    }

    /// Instantiate and run a task to accept connections, returning a shutdown
    /// signal.
    ///
    /// We do not recommend overriding this method. Doing so will opt out of
    /// the library's pubsub task system. Users overriding this method must
    /// manually handle connection tasks.
    fn serve_on_handle(
        self,
        router: crate::Router<()>,
        handle: Handle,
    ) -> impl Future<Output = Result<ServerShutdown, Self::Error>> + Send {
        async move {
            let root_tasks: TaskSet = handle.into();
            let notification_buffer_per_task = self.notification_buffer_size();

            ListenerTask {
                listener: self.make_listener().await?,
                manager: ConnectionManager {
                    next_id: 0,
                    router,
                    notification_buffer_per_task,
                    root_tasks: root_tasks.clone(),
                },
            }
            .spawn();
            Ok(root_tasks.into())
        }
    }

    /// Instantiate and run a task to accept connections, returning a shutdown
    /// signal.
    ///
    /// We do not recommend overriding this method. Doing so will opt out of
    /// the library's pubsub task system. Users overriding this method must
    /// manually handle connection tasks.
    fn serve(
        self,
        router: crate::Router<()>,
    ) -> impl Future<Output = Result<ServerShutdown, Self::Error>> + Send {
        self.serve_on_handle(router, Handle::current())
    }
}

/// A [`Listener`] accepts incoming connections and produces [`JsonSink`] and
/// [`JsonReqStream`] objects.
///
/// Typically this is done by producing a combined object with a [`Stream`] and
/// a [`Sink`], then using [`StreamExt::split`], however this trait allows for
/// more complex implementations.
///
/// It is expected that the stream or sink may have stream adapters that wrap
/// the underlying transport objects.
///
/// [`ReadJsonStream`]: https://docs.rs/alloy-transport-ipc/latest/alloy_transport_ipc/struct.ReadJsonStream.html
/// [`Sink`]: futures_util::sink::Sink
/// [`StreamExt::split`]: futures_util::stream::StreamExt::split
pub trait Listener: Send + 'static {
    /// The sink type produced by the listener.
    type RespSink: JsonSink;
    /// The stream type produced by the listener.
    type ReqStream: JsonReqStream;
    /// The error type for the listener.
    type Error: core::error::Error;

    /// Accept an inbound connection, and split it into a sink and stream.
    fn accept(
        &self,
    ) -> impl Future<Output = Result<(Self::RespSink, Self::ReqStream), Self::Error>> + Send;
}

/// A sink that accepts JSON
pub trait JsonSink: Send + 'static {
    /// Error type for the sink.
    type Error: core::error::Error + 'static;

    /// Send json to the sink.
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

/// A stream of inbound [`Bytes`] objects.
pub trait JsonReqStream: Stream<Item = Bytes> + Send + Unpin + 'static {}

impl<T> JsonReqStream for T where T: Stream<Item = Bytes> + Send + Unpin + 'static {}
