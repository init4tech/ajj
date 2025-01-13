use crate::shared::{ConnectionId, Instruction, InstructionBody, Manager, RouteTask, WriteTask};
use alloy::transports::ipc::ReadJsonStream;
use interprocess::local_socket::{
    tokio::SendHalf, traits::tokio::Listener, traits::tokio::Stream as _,
};
use std::collections::HashMap;
use tokio::{
    io::AsyncWriteExt,
    select,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

pub struct ConnectionManager {
    _shutdown: oneshot::Sender<()>,
}

/// Task accepts new connections, creates a new `IpcRouteTask` for each
/// connection,
pub struct IpcListener {
    listener: interprocess::local_socket::tokio::Listener,

    manager: Manager<SendHalf>,
}

impl IpcListener {
    pub fn spawn(self) -> JoinHandle<()> {
        let future = async move {
            let IpcListener {
                listener,
                mut manager,
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

                if let Err(err) = manager
                    .handle_new_connection(ReadJsonStream::from(recv), send)
                    .await
                {
                    tracing::error!(%err, "Failed to enroll connection, WriteTask has gone away.");
                    break;
                }
            }
        };
        tokio::spawn(future)
    }
}

/// The IpcWriteTask is responsible for writing responses to IPC sockets.
pub type IpcWriteTask = WriteTask<SendHalf>;

impl IpcWriteTask {
    /// Spawn an [`IpcWriteTask`] to write responses to IPC sockets.
    pub fn spawn(mut self) -> JoinHandle<()> {
        let future = async move {
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
                                if let Some(conn) = self.connections.get_mut(&inst.conn_id) {
                                    if let Err(e) =  conn.write_all(json.get().as_bytes()).await {
                                        tracing::error!(%e, "Failed to write to connection");
                                        break;
                                    }
                                }
                            }
                            InstructionBody::GoingAway => {
                                self.handle_going_away(inst.conn_id);
                            }
                        }
                    }
                }
            }
        };
        tokio::spawn(future)
    }
}
