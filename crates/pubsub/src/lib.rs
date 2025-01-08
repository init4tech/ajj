//! Pubsub
//!
//! This crate provides pubsub functionality for a JSON-RPC server.
//!
//! ## Overview
//!
//! The pubsub module provides a way to serve a [`Router`] over pubsub
//! connections like IPC and Websockets.
//!
//! ## Usage
//!
//! Typically users want to use a [`Connect`] implementor to create a server
//! using [`Connect::run`]. This will create a [`Listener`], and spawn tasks
//! to accept connections via that [`Listener`] and route requests.
//!
//! ### Advanced Usage
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
//! There are 3 tasks:
//! - `ListenerTask` - listens for new connections, accepts, and spawns
//!   `RouteTask` and `WriteTask` for each. A listener task is spawned for each
//!   style of connection (e.g. IPC or Websockets).
//! - `RouteTask` - Reads requests from an inbound connection, and spawns a
//!   tokio task for each request. There is 1 `RouteTask` per connection.
//! - `WriteTask` - Manages outbound connections, receives responses from the
//!   router, and writes responses to the relevant connection. There is 1
//!   `WriteTask` per connection.
//!
//! [`Router`]: router::Router
//! [`SocketAddr`]: std::net::SocketAddr
//! [`TcpListener`]: tokio::net::TcpListener

mod ipc;

mod shared;
pub use shared::ServerShutdown;

mod r#trait;
pub use r#trait::{Connect, In, JsonReqStream, JsonSink, Listener, Out};

mod ws;
pub use ws::WsJsonStream;
