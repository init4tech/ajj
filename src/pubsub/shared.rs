use core::fmt;

use crate::{
    pubsub::{In, JsonSink, Listener, Out},
    types::InboundData,
    HandlerCtx, TaskSet,
};
use serde_json::value::RawValue;
use tokio::{select, sync::mpsc, task::JoinHandle};
use tokio_stream::StreamExt;
use tracing::{debug, debug_span, error, instrument, trace, Instrument};

/// Default notification buffer size per task.
pub const DEFAULT_NOTIFICATION_BUFFER_PER_CLIENT: usize = 16;

/// Type alias for identifying connections.
pub type ConnectionId = u64;

/// Holds the shutdown signal for some server.
#[derive(Debug)]
pub struct ServerShutdown {
    pub(crate) task_set: TaskSet,
}

impl From<TaskSet> for ServerShutdown {
    fn from(task_set: TaskSet) -> Self {
        Self::new(task_set)
    }
}

impl ServerShutdown {
    /// Create a new `ServerShutdown` with the given shutdown signal and task
    /// set.
    pub fn new(task_set: TaskSet) -> Self {
        Self { task_set }
    }
}

impl Drop for ServerShutdown {
    fn drop(&mut self) {
        self.task_set.cancel();
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
    pub(crate) async fn task_future(self) {
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
    pub(crate) fn spawn(self) -> JoinHandle<Option<()>> {
        let tasks = self.manager.root_tasks.clone();
        let future = self.task_future();
        tasks.spawn(future)
    }
}

/// The `ConnectionManager` provides connections with IDs, and handles spawning
/// the [`RouteTask`] for each connection.
pub(crate) struct ConnectionManager {
    pub(crate) root_tasks: TaskSet,

    pub(crate) next_id: ConnectionId,

    pub(crate) router: crate::Router<()>,

    pub(crate) notification_buffer_per_task: usize,
}

impl ConnectionManager {
    /// Increment the connection ID counter and return an unused ID.
    fn next_id(&mut self) -> ConnectionId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Get a clone of the router.
    fn router(&self) -> crate::Router<()> {
        self.router.clone()
    }

    /// Create new [`RouteTask`] and [`WriteTask`] for a connection.
    fn make_tasks<T: Listener>(
        &self,
        conn_id: ConnectionId,
        requests: In<T>,
        connection: Out<T>,
    ) -> (RouteTask<T>, WriteTask<T>) {
        let (tx, rx) = mpsc::channel(self.notification_buffer_per_task);

        let tasks = self.root_tasks.child();

        let rt = RouteTask {
            router: self.router(),
            conn_id,
            write_task: tx,
            requests,
            tasks: tasks.clone(),
        };

        let wt = WriteTask {
            tasks,
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
    fn handle_new_connection<T: Listener>(&mut self, requests: In<T>, connection: Out<T>) {
        self.spawn_tasks::<T>(requests, connection);
    }
}

/// Task that reads requests from a stream, and routes them to the
/// [`Router`], ensures responses are sent to the [`WriteTask`].
///
/// [`Router`]: crate::Router
struct RouteTask<T: crate::pubsub::Listener> {
    /// Router for handling requests.
    pub(crate) router: crate::Router<()>,
    /// Connection ID for the connection serviced by this task.
    pub(crate) conn_id: ConnectionId,
    /// Sender to the write task.
    pub(crate) write_task: mpsc::Sender<Box<RawValue>>,
    /// Stream of requests.
    pub(crate) requests: In<T>,
    /// The task set for this connection
    pub(crate) tasks: TaskSet,
}

impl<T: crate::pubsub::Listener> fmt::Debug for RouteTask<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RouteTask")
            .field("conn_id", &self.conn_id)
            .finish_non_exhaustive()
    }
}

impl<T> RouteTask<T>
where
    T: crate::pubsub::Listener,
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
            tasks,
            ..
        } = self;

        loop {
            select! {
                biased;
                _ = write_task.closed() => {
                    debug!("WriteTask has gone away");
                    break;
                }
                item = requests.next() => {
                    let Some(item) = item else {
                        trace!("inbound read stream has closed");
                        break;
                    };

                    // If the inbound data is not currently parsable, we
                    // send an empty one it to the router, as the router
                    // enforces the specification.
                    let reqs = InboundData::try_from(item).unwrap_or_default();

                    let span = debug_span!("pubsub request handling", reqs = reqs.len());

                    let ctx =
                    HandlerCtx::new(
                        Some(write_task.clone()),
                        tasks.clone(),
                    );

                    let fut = router.handle_request_batch(ctx, reqs);
                    let write_task = write_task.clone();

                    // Acquiring the permit before spawning the task means that
                    // the write task can backpressure the route task. I.e.
                    // if the client stops accepting responses, we do not keep
                    // handling inbound requests.
                    let Ok(permit) = write_task.reserve_owned().await else {
                        tracing::error!("write task dropped while waiting for permit");
                        break;
                    };

                    // Run the future in a new task.
                    tasks.spawn(
                        async move {
                            // Send the response to the write task.
                            // we don't care if the receiver has gone away,
                            // as the task is done regardless.
                            if let Some(rv) = fut.await {
                                let _ = permit.send(
                                    rv
                                );
                            }
                        }
                        .instrument(span)
                    );
                }
            }
        }
        tasks.cancel();
    }

    /// Spawn the future produced by [`Self::task_future`].
    pub(crate) fn spawn(self) -> tokio::task::JoinHandle<Option<()>> {
        let tasks = self.tasks.clone();

        let future = self.task_future();

        tasks.spawn(future)
    }
}

/// The Write Task is responsible for writing JSON to the outbound connection.
struct WriteTask<T: Listener> {
    /// Task set
    pub(crate) tasks: TaskSet,

    /// ID of the connection.
    pub(crate) conn_id: ConnectionId,

    /// JSON to be written to the outbound connection.
    ///
    /// Dropping this channel will cause the associated [`RouteTask`] to
    /// shutdown.
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
    pub(crate) async fn task_future(self) {
        let WriteTask {
            tasks,
            mut json,
            mut connection,
            ..
        } = self;

        loop {
            select! {
                biased;

                _ = tasks.cancelled() => {
                    debug!("Shutdown signal received");
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
    pub(crate) fn spawn(self) -> tokio::task::JoinHandle<Option<()>> {
        let tasks = self.tasks.clone();
        let future = self.task_future();
        tasks.spawn(future)
    }
}
