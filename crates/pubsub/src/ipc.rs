use std::collections::HashMap;

use alloy::{
    rpc::json_rpc::{PartiallySerializedRequest, Response},
    transports::ipc::ReadJsonStream,
};
use interprocess::local_socket::{
    tokio::{RecvHalf, SendHalf},
    traits::tokio::Listener,
    traits::tokio::Stream as _,
};
use serde_json::value::RawValue;
use tokio::{
    io::AsyncWriteExt,
    select,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use tokio_stream::StreamExt;
use tracing::{info_span, Instrument};

pub type ConnectionId = u64;

pub struct ConnectionManager {
    _shutdown: oneshot::Sender<()>,
}

/// Task accepts new connections, creates a new `IpcRouteTask` for each
/// connection,
pub struct IpcListener {
    listener: interprocess::local_socket::tokio::Listener,
    next_id: ConnectionId,

    write_task: mpsc::Sender<Instruction>,

    router: router::Router<()>,
}

impl IpcListener {
    pub fn spawn(self) -> JoinHandle<()> {
        let future = async move {
            let IpcListener {
                listener,
                mut next_id,
                write_task,
                router,
            } = self;

            loop {
                let conn = match listener.accept().await {
                    Ok(conn) => conn,
                    Err(err) => {
                        tracing::error!(%err, "Failed to accept connection");
                        break;
                    }
                };

                let (recv, send) = conn.split();
                // TODO: hold and cancel tasks?
                IpcRouteTask {
                    router: router.clone(),
                    conn_id: next_id,
                    requests: ReadJsonStream::from(recv),
                    write_task: write_task.clone(),
                }
                .spawn();

                write_task
                    .send(Instruction {
                        conn_id: next_id,
                        body: InstructionBody::NewConn(send),
                    })
                    .await
                    .expect("infallible");

                next_id += 1;
            }
        };
        tokio::spawn(future)
    }
}

pub enum InstructionBody {
    /// New connection
    NewConn(SendHalf),
    /// Json should be written.
    Json(Box<RawValue>),
    /// Connection is going away, and can be removed from the table.
    GoingAway,
}

impl From<Box<RawValue>> for InstructionBody {
    fn from(json: Box<RawValue>) -> Self {
        InstructionBody::Json(json)
    }
}

pub struct Instruction {
    conn_id: ConnectionId,
    body: InstructionBody,
}

pub struct IpcRouteTask {
    router: router::Router<()>,

    conn_id: ConnectionId,

    requests: ReadJsonStream<RecvHalf, PartiallySerializedRequest>,

    write_task: mpsc::Sender<Instruction>,
}

impl IpcRouteTask {
    pub fn spawn(self) -> JoinHandle<()> {
        let future = async move {
            let IpcRouteTask {
                router,
                conn_id,
                mut requests,
                write_task,
            } = self;

            loop {
                select! {
                    biased;
                    _ = write_task.closed() => {
                        tracing::debug!("IpcWriteTask has gone away");
                        break;
                    }
                    item = requests.next() => {
                        let Some(item) = item else {
                            tracing::trace!("IPC read stream has closed");
                            let _ = write_task.send(
                                Instruction {
                                    conn_id,
                                    body: InstructionBody::GoingAway,
                                }
                            ).await;
                            break;
                        };

                        let id = item.meta.id.clone();
                        let span = tracing::debug_span!("ipc request handling", id = %id, method = item.meta.method.as_ref());
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
                                        tracing::error!(?res, %err, "Failed to serialize response");

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
        .instrument(info_span!("IpcRouteTask"));
        tokio::spawn(future)
    }
}
pub struct IpcWriteTask {
    /// Shutdown signal.
    ///
    /// Shutdowns bubble back up to [`IpcRouteTask`] and [`IpcListener`] when
    /// the write task is dropped, via the closed `inst` channel.
    shutdown: oneshot::Receiver<()>,

    inst: mpsc::Receiver<Instruction>,

    connections: HashMap<ConnectionId, SendHalf>,
}

impl IpcWriteTask {
    pub fn spawn(self) -> JoinHandle<()> {
        let future = async move {
            let IpcWriteTask {
                mut shutdown,
                mut inst,
                mut connections,
            } = self;

            loop {
                select! {
                    biased;
                    _ = &mut shutdown => {
                        tracing::debug!("Shutdown signal received");
                        break;
                    }
                    inst = inst.recv() => {
                        let Some(inst) = inst else {
                            tracing::error!("Json stream has closed");
                            break;
                        };

                        match inst.body {
                            InstructionBody::NewConn(conn) => {
                                connections.insert(inst.conn_id, conn);
                            }
                            InstructionBody::Json(json) => {
                                if let Some(conn) = connections.get_mut(&inst.conn_id) {
                                    if let Err(e) =  conn.write_all(json.get().as_bytes()).await {
                                        tracing::error!(%e, "Failed to write to connection");
                                        break;
                                    }
                                }
                            }
                            InstructionBody::GoingAway => {
                                connections.remove(&inst.conn_id);
                            }
                        }
                    }
                }
            }
        };
        tokio::spawn(future)
    }
}
