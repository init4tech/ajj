//! WebSocket connection manager for [`axum`]
//!
//! How this works:
//! `axum` does not provide a connection pattern that allows us to iplement
//! [`Listener`] or [`Connect`] directly. Instead, it uses a
//! [`WebSocketUpgrade`] to upgrade a connection to a WebSocket. This means
//! that we cannot use the [`Listener`] trait directly. Instead, we make a
//! [`AxumWsCfg`] that will be the [`State`] for our handler.
//!
//! The [`ajj_websocket`] handler serves the role of the [`Listener`] in this
//! case.
//!
//! [`Connect`]: crate::pubsub::Connect

use crate::{
    pubsub::{shared::ConnectionManager, Listener},
    Router,
};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::Response,
};
use bytes::Bytes;
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, Stream, StreamExt,
};
use serde_json::value::RawValue;
use std::{
    convert::Infallible,
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
};
use tokio::runtime::Handle;
use tracing::debug;

pub(crate) type SendHalf = SplitSink<WebSocket, Message>;
pub(crate) type RecvHalf = SplitStream<WebSocket>;

struct AxumListener;

impl Listener for AxumListener {
    type RespSink = SendHalf;

    type ReqStream = WsJsonStream;

    type Error = Infallible;

    async fn accept(&self) -> Result<(Self::RespSink, Self::ReqStream), Self::Error> {
        unreachable!()
    }
}

/// Configuration details for WebSocket connections using [`axum::extract::ws`].
///
/// The main points of configuration are:
/// - The runtime [`Handle`] on which to execute tasks, which can be set with
///   [`Self::with_handle`].
/// - The notification buffer size per client, which can be set with
///   [`Self::with_notification_buffer_per_client`]. See the [`crate::pubsub`]
///   module documentation for more details.
#[derive(Clone)]
pub struct AxumWsCfg {
    inner: Arc<ConnectionManager>,
}

impl core::fmt::Debug for AxumWsCfg {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AxumWsCfg")
            .field(
                "notification_buffer_per_client",
                &self.inner.notification_buffer_per_task,
            )
            .field("next_id", &self.inner.next_id)
            .finish()
    }
}

impl From<Router<()>> for AxumWsCfg {
    fn from(router: Router<()>) -> Self {
        Self::new(router)
    }
}

impl AxumWsCfg {
    /// Create a new [`AxumWsCfg`] with the given [`Router`].
    pub fn new(router: Router<()>) -> Self {
        Self {
            inner: ConnectionManager::new(router).into(),
        }
    }

    fn into_inner(self) -> ConnectionManager {
        match Arc::try_unwrap(self.inner) {
            Ok(inner) => inner,
            Err(arc) => ConnectionManager {
                root_tasks: arc.root_tasks.clone(),
                next_id: arc.next_id.clone(),
                router: arc.router.clone(),
                notification_buffer_per_task: arc.notification_buffer_per_task,
            },
        }
    }

    /// Set the handle on which to execute tasks.
    pub fn with_handle(self, handle: Handle) -> Self {
        Self {
            inner: self.into_inner().with_handle(handle).into(),
        }
    }

    /// Set the notification buffer size per client.
    pub fn with_notification_buffer_per_client(
        self,
        notification_buffer_per_client: usize,
    ) -> Self {
        Self {
            inner: self
                .into_inner()
                .with_notification_buffer_per_client(notification_buffer_per_client)
                .into(),
        }
    }
}

/// Axum handler for WebSocket connections. Used to serve
///
/// ```no_run
/// # #[cfg(all(feature = "axum", feature = "pubsub"))]
/// # use ajj::{Router, pubsub::{ajj_websocket, AxumWsCfg}};
/// # use std::sync::Arc;
/// # {
/// # async fn _main(router: Router<()>, axum: axum::Router<AxumWsCfg>) -> axum::Router<()>{
/// // The config object contains the tokio runtime handle, and the
/// // notification buffer size.
/// let cfg = AxumWsCfg::new(router);
///
/// axum
///     .route("/ws", axum::routing::any(ajj_websocket))
///     .with_state(cfg)
/// # }}
/// ```
pub async fn ajj_websocket(ws: WebSocketUpgrade, State(state): State<AxumWsCfg>) -> Response {
    ws.on_upgrade(move |ws| {
        let (sink, stream) = ws.split();

        state
            .inner
            .handle_new_connection::<AxumListener>(stream.into(), sink);

        async {}
    })
}

/// Simple stream adapter for extracting text from a [`WebSocket`].
#[derive(Debug)]
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
    /// Handle an incoming [`Message`]
    fn handle(&self, message: Message) -> Result<Option<Bytes>, &'static str> {
        match message {
            Message::Text(text) => Ok(Some(text.into())),
            Message::Close(Some(frame)) => {
                let s = "Received close frame with data";
                let reason = format!("{} ({})", frame.reason, frame.code);
                debug!(%reason, "{}", &s);
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
    type Item = Bytes;

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

impl crate::pubsub::JsonSink for SendHalf {
    type Error = axum::Error;

    async fn send_json(&mut self, json: Box<RawValue>) -> Result<(), Self::Error> {
        self.send(Message::text(json.get())).await
    }
}
