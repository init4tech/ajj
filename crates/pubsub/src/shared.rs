use crate::{In, Listener, Out};
use alloy::rpc::json_rpc::Response;
use serde_json::value::RawValue;
use tokio::{
    select,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use tokio_stream::StreamExt;
use tracing::{debug, debug_span, error, instrument, trace, Instrument};

/// Type alias for identifying connections.
pub type ConnectionId = u64;

/// Holds the shutdown signal for some server.
pub struct ServerShutdown {
    _shutdown: oneshot::Sender<()>,
}

pub struct ListenerTask<T: Listener> {
    listener: T,
    manager: Manager<T>,
}

impl<T> ListenerTask<T>
where
    T: Listener,
{
    /// Task future, which will be run by [`Self::spawn`].
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

            if let Err(err) = manager.handle_new_connection(req_stream, resp_sink).await {
                error!(%err, "Failed to enroll connection, WriteTask has gone away.");
                break;
            }
        }
    }

    /// Spawn the future produced by [`Self::task_future`].
    pub fn spawn(self) -> JoinHandle<()> {
        let future = self.task_future();
        tokio::spawn(future)
    }
}

/// The Manager tracks
pub struct Manager<T: Listener> {
    pub(crate) next_id: ConnectionId,
    pub(crate) write_task: mpsc::Sender<Instruction<Out<T>>>,
    pub(crate) router: router::Router<()>,
}

impl<T: Listener> Manager<T> {
    /// Increment the connection ID counter and return an unused ID.
    pub fn next_id(&mut self) -> ConnectionId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Get a clone of the write task sender.
    pub fn write_task(&self) -> mpsc::Sender<Instruction<Out<T>>> {
        self.write_task.clone()
    }

    /// Get a clone of the router.
    pub fn router(&self) -> router::Router<()> {
        self.router.clone()
    }

    /// Enroll a new connection.
    pub async fn enroll(
        &mut self,
        conn: Out<T>,
    ) -> Result<ConnectionId, mpsc::error::SendError<Instruction<Out<T>>>> {
        let id = self.next_id();
        self.write_task
            .send(Instruction {
                conn_id: id,
                body: InstructionBody::NewConn(conn),
            })
            .await?;
        Ok(id)
    }

    /// Send an instruction to the write task
    pub async fn send_instruction(
        &self,
        instruction: Instruction<Out<T>>,
    ) -> Result<(), mpsc::error::SendError<Instruction<Out<T>>>> {
        self.write_task.send(instruction).await
    }

    /// Spawn a new [`RouteTask`].
    pub fn spawn_route_task(&mut self, conn_id: ConnectionId, requests: In<T>) -> JoinHandle<()>
where {
        RouteTask::<T> {
            router: self.router.clone(),
            conn_id,
            write_task: self.write_task.clone(),
            requests,
        }
        .spawn()
    }

    /// Handle a new connection, enrolling it in the write task, and spawning
    /// its route task.
    pub async fn handle_new_connection(
        &mut self,
        requests: In<T>,
        out: Out<T>,
    ) -> Result<JoinHandle<()>, mpsc::error::SendError<Instruction<Out<T>>>> {
        let id = self.enroll(out).await?;
        Ok(self.spawn_route_task(id, requests))
    }
}

pub enum InstructionBody<Out> {
    /// New connection
    NewConn(Out),
    /// Json should be written.
    Json(Box<RawValue>),
    /// Connection is going away, and can be removed from the table.
    GoingAway,
}

impl<Out> From<Box<RawValue>> for InstructionBody<Out> {
    fn from(json: Box<RawValue>) -> Self {
        InstructionBody::Json(json)
    }
}

/// Instructions to the `WriteTask`.
pub struct Instruction<Out> {
    pub(crate) conn_id: ConnectionId,
    pub(crate) body: InstructionBody<Out>,
}

/// Task that reads requests from a stream, and routes them to the
/// [`router::Router`], and ensures responses are sent to a write task.
pub struct RouteTask<T: crate::Listener> {
    pub(crate) router: router::Router<()>,
    pub(crate) conn_id: ConnectionId,
    pub(crate) write_task: mpsc::Sender<Instruction<Out<T>>>,

    pub(crate) requests: In<T>,
}

impl<T> RouteTask<T>
where
    T: crate::Listener,
{
    /// Task future, which will be run by [`Self::spawn`].
    #[instrument(name = "RouteTask", skip(self), fields(conn_id = self.conn_id))]
    pub async fn task_future(self) {
        let RouteTask {
            router,
            conn_id,
            mut requests,
            write_task,
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
                        let _ = write_task.send(
                            Instruction {
                                conn_id,
                                body: InstructionBody::GoingAway,
                            }
                        ).await;
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
                                Instruction {
                                    conn_id,
                                    body: rv.into(),
                                }
                            ).await;
                        }
                        .instrument(span)
                    );
                }
            }
        }
    }

    /// Spawn the future produced by [`Self::task_future`].
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        let future = self.task_future();
        tokio::spawn(future)
    }
}

pub struct WriteTask<Out> {
    /// Shutdown signal.
    ///
    /// Shutdowns bubble back up to [`RouteTask`] and [`ListenerTask`] when
    /// the write task is dropped, via the closed `inst` channel.
    pub(crate) shutdown: tokio::sync::oneshot::Receiver<()>,

    pub(crate) inst: mpsc::Receiver<Instruction<Out>>,

    pub(crate) connections: std::collections::HashMap<ConnectionId, Out>,
}

impl<Out> WriteTask<Out>
where
    Out: crate::JsonSink,
{
    pub fn handle_new_conn(&mut self, conn_id: ConnectionId, conn: Out) {
        self.connections.insert(conn_id, conn);
    }

    pub fn handle_going_away(&mut self, conn_id: ConnectionId) {
        self.connections.remove(&conn_id);
    }

    /// Handle Json
    pub async fn handle_json(
        &mut self,
        conn_id: ConnectionId,
        json: Box<RawValue>,
    ) -> Result<(), <Out as crate::JsonSink>::Error> {
        if let Some(conn) = self.connections.get_mut(&conn_id) {
            conn.send_json(json).await
        } else {
            Ok(())
        }
    }

    /// Task future, which will be run by [`Self::spawn`].
    pub async fn task_future(mut self) {
        loop {
            select! {
                biased;
                _ = &mut self.shutdown => {
                    tracing::debug!("Shutdown signal received");
                    break;
                }
                inst = self.inst.recv() => {
                    let Some(inst) = inst else {
                        tracing::error!("Json stream has closed");
                        break;
                    };

                    match inst.body {
                        InstructionBody::NewConn(conn) => {
                            self.handle_new_conn(inst.conn_id, conn);
                        }
                        InstructionBody::Json(json) => {
                                if let Err(e) = self.handle_json(inst.conn_id, json).await {
                                    tracing::error!(conn_id = inst.conn_id, %e, "Failed to write to connection");
                                    continue;
                                }

                        }
                        InstructionBody::GoingAway => {
                            self.handle_going_away(inst.conn_id);
                        }
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
