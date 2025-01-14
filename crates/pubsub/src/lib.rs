//! Pubsub
//!
//! This crate provides pubsub functionality for the RPC server.
//!
//! ## Task Structure
//!
//! There are 3 tasks:
//! - [`ListenerTask`] - listens for new connections, and spawns [`RouteTask`]s
//!    for each. E.g. [`WsListenerTask`], [`IpcListenerTask`].
//! - [`RouteTask`] - reads requests from a connection, and spawns a
//!   [`tokio::task`] for each request.
//! - [`WriteTask`] - Manages outbound connections, receives responses from the
//!   router, and writes responses to the relevant connection. E.g.
//!   [`IpcWriteTask`], [`WsWriteTask`].

mod ipc;
pub use ipc::{IpcListenerTask, IpcRouteTask, IpcWriteTask};

mod shared;
pub use shared::{
    Instruction, InstructionBody, ListenerTask, Manager, RouteTask, ServerShutdown, WriteTask,
};

mod r#trait;
pub use r#trait::{Connect, In, JsonReqStream, JsonSink, Listener, Out};

mod ws;
pub use ws::{WsListenerTask, WsRouteTask, WsWriteTask};
