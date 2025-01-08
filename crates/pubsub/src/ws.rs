use alloy::rpc::json_rpc::PartiallySerializedRequest;
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, Stream, StreamExt,
};
use serde_json::value::RawValue;
use std::{
    future::Future,
    net::SocketAddr,
    pin::Pin,
    task::{ready, Context, Poll},
};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, tungstenite::protocol::Message, WebSocketStream};
use tracing::{debug, debug_span, error, trace, Instrument};

/// Sending half of a [`WebSocketStream`]
pub type SendHalf = SplitSink<WebSocketStream<TcpStream>, Message>;

/// Receiving half of a [`WebSocketStream`].
pub type RecvHalf = SplitStream<WebSocketStream<TcpStream>>;

/// Simple stream adapter for deserializing JSON from a websocket.
///
/// Malformatted JSON is dropped.
pub struct WsJsonStream {
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

    pub fn handle(
        &self,
        message: Message,
    ) -> Result<Option<PartiallySerializedRequest>, &'static str> {
        match message {
            Message::Text(text) => match self.handle_text(&text) {
                Ok(item) => Ok(Some(item)),
                Err(err) => {
                    error!(%err, "failed to deserialize message");
                    Ok(None)
                }
            },
            Message::Close(Some(frame)) => {
                let s = "Received close frame with data";
                debug!(reason = %frame, "{}", &s);
                Err(s)
            }
            Message::Close(None) => {
                let s = "WS client has gone away";
                debug!("{}", &s);
                Err(s)
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
                Err(_) => self.complete = true,
            }
        }
    }
}

impl crate::JsonSink for SendHalf {
    type Error = tokio_tungstenite::tungstenite::Error;

    async fn send_json(&mut self, json: Box<RawValue>) -> Result<(), Self::Error> {
        self.send(Message::text(json.get())).await
    }
}

impl crate::Listener for TcpListener {
    type RespSink = SendHalf;

    type ReqStream = WsJsonStream;

    type Error = tokio_tungstenite::tungstenite::Error;

    async fn accept(&self) -> Result<(Self::RespSink, Self::ReqStream), Self::Error> {
        let (stream, socket_addr) = self.accept().await?;

        let span = debug_span!("ws connection", remote_addr = %socket_addr);

        let ws_stream = accept_async(stream).instrument(span).await?;

        let (send, recv) = ws_stream.split();

        Ok((send, recv.into()))
    }
}

impl crate::Connect for SocketAddr {
    type Listener = TcpListener;
    type Error = std::io::Error;

    fn make_listener(self) -> impl Future<Output = Result<Self::Listener, Self::Error>> + Send {
        TcpListener::bind(self)
    }
}
