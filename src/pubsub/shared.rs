use crate::{
    pubsub::{In, JsonSink, Listener, Out},
    types::InboundData,
    HandlerCtx, TaskSet, TracingInfo,
};
use core::fmt;
use serde_json::value::RawValue;
use std::sync::{
    atomic::{AtomicU32, AtomicU64, Ordering},
    Arc,
};
use tokio::{pin, runtime::Handle, select, sync::mpsc, task::JoinHandle};
use tokio_stream::StreamExt;
use tokio_util::sync::WaitForCancellationFutureOwned;
use tracing::{debug, error, trace, Instrument};

/// Default notification buffer size per task.
pub const DEFAULT_NOTIFICATION_BUFFER_PER_CLIENT: usize = 16;

/// Type alias for identifying connections.
pub type ConnectionId = u64;

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
        let ListenerTask { listener, manager } = self;

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
        tasks.spawn_cancellable(future)
    }
}

/// The `ConnectionManager` provides connections with IDs, and handles spawning
/// the [`RouteTask`] for each connection.
pub(crate) struct ConnectionManager {
    pub(crate) root_tasks: TaskSet,

    pub(crate) next_id: Arc<AtomicU64>,

    pub(crate) router: crate::Router<()>,

    pub(crate) notification_buffer_per_task: usize,

    // OTEL message counters
    pub(crate) tx_msg_id: Arc<AtomicU32>,
    pub(crate) rx_msg_id: Arc<AtomicU32>,
}

impl ConnectionManager {
    /// Create a new [`ConnectionManager`] with the given [`crate::Router`].
    pub(crate) fn new(router: crate::Router<()>) -> Self {
        Self {
            root_tasks: Default::default(),
            next_id: AtomicU64::new(0).into(),
            router,
            notification_buffer_per_task: DEFAULT_NOTIFICATION_BUFFER_PER_CLIENT,
            tx_msg_id: Arc::new(AtomicU32::new(0)),
            rx_msg_id: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Set the root task set. This should generally not be used after tasks
    /// have been spawned.
    pub(crate) fn with_root_tasks(mut self, root_tasks: TaskSet) -> Self {
        self.root_tasks = root_tasks;
        self
    }

    /// Set the handle, overriding the root tasks. This should generally not be
    /// used after tasks have been spawned.
    pub(crate) fn with_handle(mut self, handle: Handle) -> Self {
        self.root_tasks = self.root_tasks.with_handle(handle);
        self
    }

    /// Set the notification buffer size per task.
    pub(crate) const fn with_notification_buffer_per_client(
        mut self,
        notification_buffer_per_client: usize,
    ) -> Self {
        self.notification_buffer_per_task = notification_buffer_per_client;
        self
    }

    /// Increment the connection ID counter and return an unused ID.
    fn next_id(&self) -> ConnectionId {
        self.next_id.fetch_add(1, Ordering::Relaxed)
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
            rx_msg_id: self.rx_msg_id.clone(),
        };

        let wt = WriteTask {
            tasks,
            conn_id,
            items: rx,
            connection,
            tx_msg_id: self.tx_msg_id.clone(),
        };

        (rt, wt)
    }

    /// Spawn a new [`RouteTask`] and [`WriteTask`] for a connection.
    fn spawn_tasks<T: Listener>(&self, requests: In<T>, connection: Out<T>) {
        let conn_id = self.next_id();
        let (rt, wt) = self.make_tasks::<T>(conn_id, requests, connection);
        rt.spawn();
        wt.spawn();
    }

    /// Handle a new connection, enrolling it in the write task, and spawning
    /// its route task.
    pub(crate) fn handle_new_connection<T: Listener>(&self, requests: In<T>, connection: Out<T>) {
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
    pub(crate) write_task: mpsc::Sender<WriteItem>,
    /// Stream of requests.
    pub(crate) requests: In<T>,
    /// The task set for this connection
    pub(crate) tasks: TaskSet,

    /// Counter for OTEL messages received.
    pub(crate) rx_msg_id: Arc<AtomicU32>,
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
    pub(crate) async fn task_future(self, cancel: WaitForCancellationFutureOwned) {
        let RouteTask {
            router,
            mut requests,
            write_task,
            tasks,
            rx_msg_id,
            ..
        } = self;

        // The write task is responsible for waiting for its children
        let children = tasks.child();

        pin!(cancel);

        loop {
            select! {
                biased;
                _ = &mut cancel => {
                    debug!("RouteTask cancelled");
                    break;
                }
                _ = write_task.closed() => {
                    debug!("WriteTask has gone away");
                    break;
                }
                item = requests.next() => {
                    let Some(item) = item else {
                        trace!("inbound read stream has closed");
                        break;
                    };

                    let item_bytes = item.len();

                    // If the inbound data is not currently parsable, we
                    // send an empty one it to the router, as the router
                    // enforces the specification.
                    let reqs = InboundData::try_from(item).unwrap_or_default();

                    // Acquiring the permit before spawning the task means that
                    // the write task can backpressure the route task. I.e.
                    // if the client stops accepting responses, we do not keep
                    // handling inbound requests.
                    let Ok(permit) = write_task.clone().reserve_owned().await else {
                        error!("write task dropped while waiting for permit");
                        break;
                    };

                     // This span is populated with as much detail as
                     // possible, and then given to the Handler ctx. It
                     // will be populated with request-specific details
                     // (e.g. method) during ctx instantiation.
                    let tracing = TracingInfo { service: router.service_name(), request_span: request_span!(name: "ajj.pubsub.RouteTask::call", router: &router, with_notifications) };

                    let ctx =
                    HandlerCtx::new(
                        Some(write_task.clone()),
                        children.clone(),
                        tracing,
                    );

                    let span = ctx.span().clone();
                    span.in_scope(|| {
                        message_event!(
                            @received,
                            counter: &rx_msg_id,
                            bytes: item_bytes,
                        );
                    });

                    // Run the future in a new task.
                    let fut = router.handle_request_batch(ctx, reqs);

                    children.spawn_cancellable(
                        async move {
                            // Send the response to the write task.
                            // we don't care if the receiver has gone away,
                            // as the task is done regardless.
                            if let Some(json) = fut.await {
                                let _ = permit.send(
                                    WriteItem { span, json }
                                );
                            }
                        }
                    );
                }
            }
        }
        children.shutdown().await;
    }

    /// Spawn the future produced by [`Self::task_future`].
    pub(crate) fn spawn(self) -> tokio::task::JoinHandle<()> {
        let tasks = self.tasks.clone();

        let future = move |cancel| self.task_future(cancel);

        tasks.spawn_graceful(future)
    }
}

/// An item to be written to an outbound JSON pubsub stream.
#[derive(Debug, Clone)]
pub(crate) struct WriteItem {
    pub(crate) span: tracing::Span,
    pub(crate) json: Box<RawValue>,
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
    pub(crate) items: mpsc::Receiver<WriteItem>,

    /// Outbound connections.
    pub(crate) connection: Out<T>,

    /// Counter for OTEL messages sent.
    pub(crate) tx_msg_id: Arc<AtomicU32>,
}

impl<T: Listener> WriteTask<T> {
    /// Task future, which will be run by [`Self::spawn`].
    ///
    /// This is a simple loop, that reads instructions from the instruction
    /// channel, and acts on them. It handles JSON messages, and going away
    /// instructions. It also listens for the global shutdown signal from the
    /// [`ServerShutdown`] struct.
    ///
    /// [`ServerShutdown`]: crate::pubsub::ServerShutdown
    pub(crate) async fn task_future(self) {
        let WriteTask {
            tasks,
            mut items,
            mut connection,
            tx_msg_id,
            ..
        } = self;

        loop {
            select! {
                biased;

                _ = tasks.cancelled() => {
                    debug!("Shutdown signal received");
                    break;
                }
                item = items.recv() => {
                    let Some(WriteItem { span, json }) = item else {
                        tracing::error!("Json stream has closed");
                        break;
                    };
                    span.in_scope(|| {
                        message_event!(
                            @sent,
                            counter: &tx_msg_id,
                            bytes: json.get().len(),
                        );
                    });

                    if let Err(err) = connection.send_json(json).instrument(span).await {
                        debug!(%err, conn_id = self.conn_id, "Failed to send json");
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
        tasks.spawn_cancellable(future)
    }
}
