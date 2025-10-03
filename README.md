# ajj: A Json-RPC Router

A general-purpose, batteries-included [JSON-RPC 2.0] router, inspired by
[axum]'s routing system.

## High-level features

ajj aims to provide simple, flexible, and ergonomic routing for JSON-RPC.

- Copy-free routing of JSON-RPC requests to method handlers.
- No-boilerplate method handlers. Just write an async function.
- Support for pubsub-style notifications.
- Built-in support for axum, and tower's middleware and service ecosystem.
- Basic built-in pubsub server implementations for WS and IPC.
- Connection-oriented task management automatically cancels tasks on client
  disconnect.

## Concepts

- A **Handler** is a function that processes some JSON input, and succeeds or
  fails with some data.
- A **Method** is a handler with an associated name, which is used by the
  incoming [request] to identify a specific **Handler**.
- A **Router** is a collection of **Methods**.
- A **Server** exposes a **Router** over some transport.

## Basic Usage

See the [crate documentation on docs.rs] for more detailed examples.

```rust
use ajj::{Router};

// Provide methods called "double" and "add" to the router.
let router = Router::<u64>::new()
  // "double" returns the double of the request's parameter.
  .route("double", |params: u64| async move {
    Ok::<_, ()>(params * 2)
  })
  // "add" returns the sum of the request's parameters and the router's stored
  // state.
  .route("add", |params: u64, state: u64| async move {
    Ok::<_, ()>(params + state)
  })
  // The router is provided with state, and is now ready to handle requests.
  .with_state::<()>(3u64);
```

## Feature flags

- `axum` - implements the `tower::Service` trait for `Router`, allowing it to
  be used as an [axum]() handler.
- `pubsub` - adds traits and tasks for serving the router over streaming
  interfaces.
- `ws` - adds implementations of the `pubsub` traits for
  [tokio-tungstenite].
- `ipc` - adds implementations of the `pubsub` traits for
  [interprocess `local_sockets`].

## Serving the Router

We recommend [axum] for serving the router over HTTP. The `Router` provides an
`into_axum(path: &str)` method to instantiate a new [`axum::Router`], and
register the router to handle requests.

For WS and IPC connections, the `pubsub` module provides implementations of the
[`Connect`] trait for [`std::net::SocketAddr`] to create simple WS servers, and
[`interprocess::local_socket::ListenerOptions`] to create simple IPC servers.
Users with more complex needs should provide their own [`Connect`]
implementations.

See the [crate documentation on docs.rs] for more detailed examples.

## Specification Complinace

`ajj` aims to be fully compliant with the [JSON-RPC 2.0] specification. If any
issues are found, please [open an issue]!

`ajj` produces [`tracing`] spans and events that meet the [OpenTelemetry
semantic conventions] for JSON-RPC servers with the following exception:

- The `server.address` attribute is NOT set, as the server address is not always
  known to the ajj system.
-

## Note on code provenance

Some code in this project has been reproduced or adapted from other projects.
Files containing that code contain a note at the bottom of the file indicating
the original source, and containing relevant license information. Code has been
reproduced from the following projects, and we are grateful for their work:

- [axum] - for the `Router` struct and `Handler` trait, as well as for the
  internal method, route, and handler representations.
- [alloy] - for the `ResponsePayload` and associated structs, the `RpcSend` and
  `RpcRecv` family of traits, and the `JsonReadStream` in the `ipc` module.

[crate documentation on docs.rs]: https://docs.rs/ajj/latest/ajj/
[`Connect`]: https://docs.rs/ajj/latest/ajj/pubsub/trait.Connect.html
[JSON-RPC 2.0]: https://www.jsonrpc.org/specification
[request]: https://www.jsonrpc.org/specification#request_object
[axum]: https://docs.rs/axum/latest/axum/
[tokio-tungstenite]: https://docs.rs/tokio-tungstenite/latest/tokio_tungstenite/
[`axum::Router`]: https://docs.rs/axum/latest/axum/struct.Router.html
[interprocess `local_sockets`]: https://docs.rs/interprocess/latest/interprocess/local_socket/tokio/index.html
[`interprocess::local_socket::ListenerOptions`]: https://docs.rs/interprocess/latest/interprocess/local_socket/struct.ListenerOptions.html
[std::net::SocketAddr]: https://doc.rust-lang.org/std/net/enum.SocketAddr.html
[alloy]: https://docs.rs/alloy/latest/alloy/
[open an issue]: https://github.com/init4tech/ajj/issues/new
[OpenTelemetry semantic conventions]: https://opentelemetry.io/docs/specs/semconv/rpc/json-rpc/
[`tracing`]: https://docs.rs/tracing/latest/tracing/
