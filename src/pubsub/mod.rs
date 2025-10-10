//! Pubsub serving utils for [`Router`]s.
//!
//! This module provides pubsub functionality for serving [`Router`]s over
//! various connection types. Built-in support is provided for IPC and
//! Websockets, and a trait system is provided for custom connection types.
//!
//! ## Overview
//!
//! The pubsub module provides a way to serve a [`Router`] over pubsub
//! connections like IPC and Websockets.
//!
//! ## Usage
//!
//! Typically users want to use a [`Connect`] implementor to create a server
//! using [`Connect::serve`]. This will create a [`Listener`]. [`Listener`]s
//! manage accepting client connections, and spawn tasks to route requests for
//! each connection.
//!
//! ### Advanced Usage
//!
//! ## Backpressure and buffer saturation
//!
//! Backpressure is managed by a notification buffer. This buffer is allocated
//! on a per-client basis, and contains both notifications, and responses. The
//! buffer is filled when the client is not reading from the connection, and
//! the server will stop processing requests from that client until the buffer
//! has room.
//!
//! The buffer size is set by the [`Connect::notification_buffer_size`] method,
//! and defaults to [`DEFAULT_NOTIFICATION_BUFFER_PER_CLIENT`]. It is
//! recommended to set this value based on the expected size of notifications
//! and responses in your RPC server, as setting it too high may lead to server
//! resource exhaustion.
//!
//! The server contains two tasks per connection: a `RouteTask` and a
//! `WriteTask`. The `WriteTask` is responsible for notifying clients by pulling
//! items from the notification buffer, and writing them to the connection. The
//! `RouteTask` is responsible for reading requests from the connection, and
//! spawning tasks to handle each request. The `RouteTask` will always invoke
//! [`Sender::reserve_owned`] before invoking a [`Handler`]. This ensures that
//! the server will not process requests if the notification buffer is full.
//!
//! When [`Handler`]s send notifications via the [`HandlerCtx`], they SHOULD
//! `await` on [`Sender::send`]. This queues them for access to the notification
//! buffer. If the buffer is full, the `await` will backpressure the handler
//! until the buffer has room. However, if many handlers are consuming the
//! buffer, it will ALSO backpressure the `RouteTask` from reading requests from
//! the connection. [`Handler`] implementers should be aware of this.
//!
//! #### Custom Connector
//!
//! [`Connect`] has been implemented for
//! [`interprocess::local_socket::ListenerOptions`] (producing a
//! [`interprocess::local_socket::tokio::Listener`]),
//! and for [`SocketAddr`] (producing a [`TcpListener`]).
//!
//! Custom [`Connect`] implementors can configure the listener in any way they
//! need. This is useful for (e.g.) configuring network or security policies on
//! the inbound TLS connection.
//!
//! #### Custom Listener
//!
//! If you need more control over the server, you can create your own
//! [`Listener`]. This is useful if you need to customize the connection
//! handling, or if you need to use a different connection type.
//!
//! [`Listener`]'s associated stream and sink types are used to read requests
//! and write responses. These types must implement [`JsonReqStream`] and
//! [`JsonSink`] respectively.
//!
//! ## Internal Structure
//!
//! There are 3 tasks:x
//! - `ListenerTask` - listens for new connections, accepts, and spawns
//!   `RouteTask` and `WriteTask` for each. A listener task is spawned for each
//!   style of connection (e.g. IPC or Websockets).
//! - `RouteTask` - Reads requests from an inbound connection, and spawns a
//!   tokio task for each request. There is 1 `RouteTask` per connection.
//! - `WriteTask` - Manages outbound connections, receives responses from the
//!   router, and writes responses to the relevant connection. There is 1
//!   `WriteTask` per connection.
//!
//! [`Router`]: crate::Router
//! [`SocketAddr`]: std::net::SocketAddr
//! [`TcpListener`]: tokio::net::TcpListener
//! [`Sender::reserve_owned`]: tokio::sync::mpsc::Sender::reserve_owned
//! [`Sender::send`]: tokio::sync::mpsc::Sender::send
//! [`Handler`]: crate::Handler
//! [`HandlerCtx`]: crate::HandlerCtx

#[cfg(feature = "ipc")]
mod ipc_inner;
#[cfg(feature = "ipc")]
#[doc(hidden)]
// Re-exported for use in tests
pub use ipc_inner::ReadJsonStream;

/// IPC support via interprocess local sockets.
#[cfg(feature = "ipc")]
pub mod ipc {
    use std::ffi::OsStr;

    pub use interprocess::local_socket::{self as local_socket, Listener, ListenerOptions, Name};

    /// Convenience function to convert an [`OsStr`] to a local socket [`Name`]
    /// in a platform-safe way.
    pub fn to_name(path: &OsStr) -> std::io::Result<local_socket::Name<'_>> {
        if cfg!(windows) && !path.as_encoded_bytes().starts_with(br"\\.\pipe\") {
            local_socket::ToNsName::to_ns_name::<local_socket::GenericNamespaced>(path)
        } else {
            local_socket::ToFsName::to_fs_name::<local_socket::GenericFilePath>(path)
        }
    }
}

mod shared;
pub(crate) use shared::WriteItem;
pub use shared::{ConnectionId, DEFAULT_NOTIFICATION_BUFFER_PER_CLIENT};

mod shutdown;
pub use shutdown::ServerShutdown;

mod r#trait;
pub use r#trait::{Connect, In, JsonReqStream, JsonSink, Listener, Out};

#[cfg(feature = "ws")]
mod ws;

#[cfg(feature = "axum")]
mod axum;
#[cfg(feature = "axum")]
pub use axum::{ajj_websocket, AxumWsCfg};
