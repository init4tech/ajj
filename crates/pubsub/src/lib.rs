use alloy::rpc::json_rpc::{PartiallySerializedRequest, Response};
use alloy::transports::ipc::ReadJsonStream;
use interprocess::local_socket::tokio::{RecvHalf, SendHalf};
use serde_json::value::RawValue;
use tokio::{io::AsyncWriteExt, sync::mpsc};
use tokio_stream::StreamExt;
use tracing::Instrument;

/// Connection Manager
pub struct IpcConnectionManager {
    /// Shutdown handle for [`IpcWriteTask`].
    ///
    /// When this is dropped, the write task will shut down. This will cause
    /// [`IpcReadTask`] to shut down as well, as the [`mpsc::Receiver`] held
    /// by the write task will be dropped.
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

/// Task that reads requests from the IPC socket, and routes them to the
/// [`router::Router`].
pub struct IpcReadTask {
    router: router::Router<()>,
    write_sink: mpsc::Sender<Box<RawValue>>,
    ipc_stream: RecvHalf,
}

impl IpcReadTask {
    /// Spawn an [`IpcReadTask`] to read requests from the IPC socket, and send
    /// them a [`router::Router`] for processing.
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let IpcReadTask {
                router,
                write_sink,
                ipc_stream,
            } = self;

            let mut ipc_stream =
                ReadJsonStream::<_, PartiallySerializedRequest>::from(ipc_stream).fuse();

            loop {
                tokio::select! {
                    biased;
                    _ = write_sink.closed() => {
                        tracing::debug!("IpcWriteTask has gone away");
                        break;
                    }
                    item = ipc_stream.next() => {
                        // If the stream has ended, break the loop.
                        let Some(item) = item else {
                            tracing::debug!("IPC read stream has closed");
                            break;
                        };

                        let id = item.meta.id.clone();
                        let span = tracing::debug_span!("ipc request handling", id = %id, method = item.meta.method.as_ref());

                        // Spawn a task to handle the request
                        let fut = router.handle_request(item);

                        // Pass it a copy of the Sender
                        let write_sink = write_sink.clone();

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
                                let _ = write_sink.send(rv).await;
                            }
                            .instrument(span)
                        );
                    }

                }
            }
        })
    }
}

/// Simple task that writes responses to the IPC socket.
pub struct IpcWriteTask {
    shutdown: tokio::sync::oneshot::Receiver<()>,
    json: mpsc::Receiver<Box<RawValue>>,
    ipc_writer: SendHalf,
}

impl IpcWriteTask {
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let IpcWriteTask {
                mut shutdown,
                mut json,
                mut ipc_writer,
            } = self;

            loop {
                tokio::select! {
                    biased;
                    _ = &mut shutdown => {
                        break;
                    }
                    item = json.recv() => {
                        let Some(item) = item else {
                            tracing::debug!("Backend has gone away");
                            break;
                        };

                        if let Err(err) = ipc_writer.write_all(item.get().as_bytes()).await {
                            tracing::error!(%err, "Failed to write to IPC socket");
                            break;
                        }
                    },
                }
            }
        })
    }
}
