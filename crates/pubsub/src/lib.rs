//! Pubsub
//!
//! This crate provides pubsub functionality for the RPC server.
//!
//! ## Task Structure
//!
//! There are 3 tasks:
//! - `Listener` - listens for new connections, and spawns `RouteTask`s for
//!    each. E.g. [`WsListener`], [`IpcListener`].
//! - `RouteTask` - reads requests from a connection, and spawns a
//!   [`tokio::task`] for each request. This is completely generic over the
//!   stream type.
//! - `WriteTask` - Manages outbound connections, receives responses from the
//!   router, and writes responses to the relevant connection. E.g.
//!   [`IpcWriteTask`], [`WsWriteTask`].

mod ipc;
pub use ipc::{IpcListener, IpcWriteTask};

mod shared;
pub use shared::{Instruction, InstructionBody, Manager, RouteTask, ServerShutdown, WriteTask};

mod ws;
pub use ws::{WsListener, WsWriteTask};
