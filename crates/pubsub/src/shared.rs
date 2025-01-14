use crate::{In, JsonSink, Listener, Out};
use alloy::rpc::json_rpc::Response;
use serde_json::value::RawValue;
use tokio::{
    select,
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};
use tokio_stream::StreamExt;
use tracing::{debug, debug_span, error, instrument, trace, Instrument};

/// Default instruction buffer size per task.
pub const DEFAULT_INSTRUCTION_BUFFER_PER_TASK: usize = 15;

/// Type alias for identifying connections.
pub type ConnectionId = u64;

/// Holds the shutdown signal for some server.
pub struct ServerShutdown {
    pub(crate) _shutdown: watch::Sender<()>,
}

impl From<watch::Sender<()>> for ServerShutdown {
    fn from(sender: watch::Sender<()>) -> Self {
        Self { _shutdown: sender }
    }
}

/// The `ListenerTask` listens for new connections, and spawns `RouteTask`s for
/// each.
pub(crate) struct ListenerTask<T: Listener> {
    pub(crate) listener: T,
    pub(crate) manager: ConnectionManager,
}

impl<T> ListenerTask<T>
where
    T: Listener,
{
    /// Task future, which will be run by [`Self::spawn`].
    ///
    /// This future is a simple loop that accepts new connections, and uses
    /// the [`ConnectionManager`] to handle them.
    pub async fn task_future(self) {
        let ListenerTask {
            listener,
            mut manager,
        } = self;

        loop {
            let (resp_sink, req_stream) = match listener.accept().await {
                Ok((resp_sink, req_stream)) => (resp_sink, req_stream),
                Err(err) => {
                    error!(%err, "Failed to accept connection");
                    // TODO: should these errors be considered persistent?
                    continue;
                }
            };

            manager.handle_new_connection::<T>(req_stream, resp_sink);
        }
    }

    /// Spawn the future produced by [`Self::task_future`].
    pub fn spawn(self) -> JoinHandle<()> {
        let future = self.task_future();
        tokio::spawn(future)
    }
}

/// The `ConnectionManager` provides connections with IDs, and handles spawning
/// the [`RouteTask`] for each connection.
pub(crate) struct ConnectionManager {
    pub(crate) shutdown: watch::Receiver<()>,

    pub(crate) next_id: ConnectionId,

    pub(crate) router: router::Router<()>,

    pub(crate) instruction_buffer_per_task: usize,
}

impl ConnectionManager {
    /// Increment the connection ID counter and return an unused ID.
    pub fn next_id(&mut self) -> ConnectionId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Get a clone of the router.
    pub fn router(&self) -> router::Router<()> {
        self.router.clone()
    }

    /// Create new [`RouteTask`] and [`WriteTask`] for a connection.
    fn make_tasks<T: Listener>(
        &self,
        conn_id: ConnectionId,
        requests: In<T>,
        connection: Out<T>,
    ) -> (RouteTask<T>, WriteTask<T>) {
        let (tx, rx) = mpsc::channel(self.instruction_buffer_per_task);

        let (gone_tx, gone_rx) = oneshot::channel();

        let rt = RouteTask {
            router: self.router(),
            conn_id,
            write_task: tx,
            requests,
            gone: gone_tx,
        };

        let wt = WriteTask {
            shutdown: self.shutdown.clone(),
            gone: gone_rx,
            conn_id,
            json: rx,
            connection,
        };

        (rt, wt)
    }

    /// Spawn a new [`RouteTask`] and [`WriteTask`] for a connection.
    fn spawn_tasks<T: Listener>(&mut self, requests: In<T>, connection: Out<T>) {
        let conn_id = self.next_id();
        let (rt, wt) = self.make_tasks::<T>(conn_id, requests, connection);
        rt.spawn();
        wt.spawn();
    }

    /// Handle a new connection, enrolling it in the write task, and spawning
    /// its route task.
    pub fn handle_new_connection<T: Listener>(&mut self, requests: In<T>, connection: Out<T>) {
        self.spawn_tasks::<T>(requests, connection);
    }
}

/// Task that reads requests from a stream, and routes them to the
/// [`router::Router`], ensures responses are sent to the [`WriteTask`].
pub struct RouteTask<T: crate::Listener> {
    /// Router for handling requests.
    pub(crate) router: router::Router<()>,
    /// Connection ID for the connection serviced by this task.
    pub(crate) conn_id: ConnectionId,
    /// Sender to the write task.
    pub(crate) write_task: mpsc::Sender<Box<RawValue>>,
    /// Stream of requests.
    pub(crate) requests: In<T>,
    /// Sender to the [`WriteTask`], to notify it that this task is done.
    pub(crate) gone: oneshot::Sender<()>,
}

impl<T> RouteTask<T>
where
    T: crate::Listener,
{
    /// Task future, which will be run by [`Self::spawn`].
    ///
    /// This future is a simple loop, which reads requests from the stream,
    /// and routes them to the router. For each request, a new task is spawned
    /// to handle the request, and given a sender to the [`WriteTask`]. This
    /// ensures that requests can be handled concurrently.
    #[instrument(name = "RouteTask", skip(self), fields(conn_id = self.conn_id))]
    pub async fn task_future(self) {
        let RouteTask {
            router,
            mut requests,
            write_task,
            gone,
            ..
        } = self;

        loop {
            select! {
                biased;
                _ = write_task.closed() => {
                    debug!("IpcWriteTask has gone away");
                    break;
                }
                item = requests.next() => {
                    let Some(item) = item else {
                        trace!("IPC read stream has closed");
                        break;
                    };

                    let id = item.meta.id.clone();
                    let span = debug_span!("ipc request handling", id = %id, method = item.meta.method.as_ref());
                    let fut = router.handle_request(item);
                    let write_task = write_task.clone();

                        // Run the future in a new task.
                        tokio::spawn(
                        async move {
                            // Run the request handler and serialize the
                            // response.
                            let res = fut.await.expect("infallible");
                            let ser = match serde_json::to_string(&res) {
                                Ok(res) => res,
                                Err(err) => {
                                    error!(?res, %err, "Failed to serialize response");

                                    serde_json::to_string(&Response::<(),()>::internal_error_message(id, "Failed to serialize response".into())).expect("double serialization error")


                                }
                            };
                            let rv = RawValue::from_string(ser).expect("known to be valid JSON");
                            // Send the response to the write task.
                            // we don't care if the receiver has gone away,
                            // as the task is done regardless.
                            let _ = write_task.send(
                                rv
                            ).await;
                        }
                        .instrument(span)
                    );
                }
            }
        }
        // No funny business. Drop the gone signal.
        drop(gone);
    }

    /// Spawn the future produced by [`Self::task_future`].
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        let future = self.task_future();
        tokio::spawn(future)
    }
}
pub struct WriteTask<T: Listener> {
    /// Shutdown signal.
    ///
    /// Shutdowns bubble back up to [`RouteTask`] and [`ListenerTask`] when
    /// the write task is dropped, via the closed `inst` channel.
    pub(crate) shutdown: watch::Receiver<()>,

    /// Signal that the connection has gone away.
    pub(crate) gone: oneshot::Receiver<()>,

    /// ID of the connection.
    pub(crate) conn_id: ConnectionId,

    /// JSON to be written to the outbound connection.
    pub(crate) json: mpsc::Receiver<Box<RawValue>>,

    /// Outbound connections.
    pub(crate) connection: Out<T>,
}

impl<T: Listener> WriteTask<T> {
    /// Task future, which will be run by [`Self::spawn`].
    ///
    /// This is a simple loop, that reads instructions from the instruction
    /// channel, and acts on them. It handles JSON messages, and going away
    /// instructions. It also listens for the global shutdown signal from the
    /// [`ServerShutdown`] struct.
    #[instrument(skip(self), fields(conn_id = self.conn_id))]
    pub async fn task_future(self) {
        let WriteTask {
            mut shutdown,
            mut gone,
            mut json,
            mut connection,
            ..
        } = self;
        shutdown.mark_unchanged();
        loop {
            select! {
                biased;
                _ = &mut gone => {
                    debug!("Connection has gone away");
                    break;
                }
                _ = shutdown.changed() => {
                    debug!("shutdown signal received");
                    break;
                }
                json = json.recv() => {
                    let Some(json) = json else {
                        tracing::error!("Json stream has closed");
                        break;
                    };
                    if let Err(err) = connection.send_json(json).await {
                        debug!(%err, "Failed to send json");
                        break;
                    }
                }
            }
        }
    }

    /// Spawn the future produced by [`Self::task_future`].
    pub fn spawn(self) -> JoinHandle<()> {
        tokio::spawn(self.task_future())
    }
}
