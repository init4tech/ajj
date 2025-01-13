use crate::{shared::Manager, InstructionBody, WriteTask};
use alloy::rpc::json_rpc::PartiallySerializedRequest;
use futures_util::{
    stream::{SplitSink, SplitStream},
    Sink, SinkExt, Stream, StreamExt,
};
use serde_json::value::RawValue;
use std::{
    pin::Pin,
    task::ready,
    task::{Context, Poll},
};
use tokio::{
    net::{TcpListener, TcpStream},
    select,
    task::JoinHandle,
};
use tokio_tungstenite::{accept_async, tungstenite::protocol::Message, WebSocketStream};
use tracing::{debug, debug_span, error, trace, Instrument};

type SendHalf = SplitSink<WebSocketStream<TcpStream>, Message>;
type RecvHalf = SplitStream<WebSocketStream<TcpStream>>;

/// Listner for WebSocket connections.
pub struct WsListener {
    listener: TcpListener,

    manager: Manager<WsJsonSink>,
}

impl WsListener {
    pub fn spawn(self) -> JoinHandle<()> {
        let future = async move {
            let WsListener {
                listener,
                mut manager,
            } = self;
            loop {
                let Ok((stream, socket_addr)) = listener.accept().await else {
                    error!("Failed to accept connection");
                    // TODO: should these errors be considered persistent?
                    continue;
                };

                let span = debug_span!("ws connection", remote_addr = %socket_addr);

                let ws_stream = match accept_async(stream).instrument(span).await {
                    Ok(ws_stream) => ws_stream,
                    Err(err) => {
                        // TODO: should these errors be considered persistent?
                        debug!(%err, "Failed to accept WebSocket connection");
                        continue;
                    }
                };

                let (send, recv) = ws_stream.split();

                let recv = WsJsonStream::from(recv);

                if let Err(err) = manager.handle_new_connection(recv, send.into()).await {
                    error!(%err, "Failed to enroll connection, WriteTask has gone away.");
                    break;
                }
            }
        };
        tokio::spawn(future)
    }
}

#[pin_project::pin_project]
pub struct WsJsonSink {
    #[pin]
    inner: SendHalf,
}

impl From<SendHalf> for WsJsonSink {
    fn from(inner: SendHalf) -> Self {
        Self { inner }
    }
}

impl Sink<Box<RawValue>> for WsJsonSink {
    type Error = <SendHalf as Sink<Message>>::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Box<RawValue>) -> Result<(), Self::Error> {
        self.project().inner.start_send(Message::text(item.get()))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_close(cx)
    }
}

struct WsJsonStream {
    inner: RecvHalf,
    complete: bool,
}

impl From<RecvHalf> for WsJsonStream {
    fn from(inner: RecvHalf) -> Self {
        Self {
            inner,
            complete: false,
        }
    }
}

impl WsJsonStream {
    pub fn handle_text(&self, text: &str) -> Result<PartiallySerializedRequest, serde_json::Error> {
        trace!(%text, "received message from websocket");

        serde_json::from_str(text).inspect_err(|err| error!(%err, "failed to deserialize message"))
    }

    pub fn handle(&self, message: Message) -> Result<Option<PartiallySerializedRequest>, ()> {
        match message {
            Message::Text(text) => match self.handle_text(&text) {
                Ok(item) => Ok(Some(item)),
                Err(err) => {
                    error!(%err, "failed to deserialize message");
                    Ok(None)
                }
            },
            Message::Close(frame) => {
                if frame.is_some() {
                    error!(?frame, "Received close frame with data");
                } else {
                    error!("WS server has gone away");
                }
                Err(())
            }
            _ => Ok(None),
        }
    }
}

impl Stream for WsJsonStream {
    type Item = PartiallySerializedRequest;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            if self.complete {
                return Poll::Ready(None);
            }

            let Some(Ok(msg)) = ready!(self.inner.poll_next_unpin(cx)) else {
                self.complete = true;
                return Poll::Ready(None);
            };

            match self.handle(msg) {
                Ok(Some(item)) => return Poll::Ready(Some(item)),
                Ok(None) => continue,
                Err(()) => self.complete = true,
            }
        }
    }
}

/// Task for writing JSON to WS connections.
pub type WsWriteTask = WriteTask<WsJsonSink>;

impl WsWriteTask {
    pub fn spawn(mut self) -> JoinHandle<()> {
        let future = async move {
            loop {
                select! {
                    biased;
                    _ = &mut self.shutdown => {
                        debug!("Shutdown signal received");
                        break;
                    }
                    inst = self.inst.recv() => {
                        let Some(inst) = inst else {
                            error!("Json stream has closed");
                            break;
                        };

                        let conn_id = inst.conn_id;

                        match inst.body {
                            InstructionBody::NewConn(conn) => {
                                self.handle_new_conn(conn_id, conn);
                            }
                            InstructionBody::Json(json) => {
                                if let Some(conn) = self.connections.get_mut(&conn_id) {
                                    if let Err(err) = conn.send(json).await {
                                        error!(conn_id, %err, "Failed to send json");
                                    }
                                }
                            }
                            InstructionBody::GoingAway => {
                                self.handle_going_away(conn_id);
                            }
                        }
                    }
                }
            }
        };
        tokio::spawn(future)
    }
}
