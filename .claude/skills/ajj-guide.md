# ajj Crate Guide

## Overview

`ajj` is a JSON-RPC 2.0 router library inspired by axum's routing system. It
provides copy-free routing, zero-boilerplate handlers, pubsub notifications,
and built-in axum/tower integration. Version 0.4.1, MSRV 1.81.

## Architecture

The core abstraction pipeline: **Handler -> Method -> Router -> Server**.

### Source Layout

```
src/
  lib.rs          - Crate root, public re-exports, lint directives
  router.rs       - Router<S> and RouterInner<S>
  primitives.rs   - MethodId, RpcSend, RpcRecv, RpcBorrow, RpcObject
  error.rs        - RegistrationError
  macros.rs       - Internal utility macros (tap_inner!, map_inner!, panic_on_err!, impl_handler_call!)
  metrics.rs      - OpenTelemetry metrics instrumentation
  tasks.rs        - TaskSet for connection-scoped task management
  axum.rs         - Axum HTTP integration (feature-gated)
  routes/
    mod.rs        - Route, re-exports
    handler.rs    - Handler trait with blanket impls, Params<T>, State<S>
    ctx.rs        - HandlerCtx, HandlerArgs, TracingInfo, NotifyError
    erased.rs     - Type erasure for handlers (ErasedIntoRoute, BoxedIntoRoute)
    future.rs     - RouteFuture, BatchFuture
    method.rs     - Method<S> enum (Ready/Needs)
  types/
    mod.rs        - Module re-exports
    req.rs        - Request parsing from bytes
    batch.rs      - Batch request handling
    error.rs      - RequestError types
    macros.rs     - Type helper macros
    resp/
      mod.rs      - Response types
      payload.rs  - ResponsePayload<T, E> enum
      ser.rs      - Response serialization
  pubsub/         - (feature-gated: pubsub, ws, ipc)
    mod.rs        - Module root, Connect/Listener traits
    trait.rs      - Connect and Listener trait definitions
    shared.rs     - Shared pubsub types
    shutdown.rs   - ServerShutdown
    axum.rs       - Axum WebSocket integration
    ws.rs         - tokio-tungstenite WebSocket support
    ipc_inner.rs  - interprocess IPC support
examples/
  axum.rs         - HTTP POST + WebSocket serving
  ipc.rs          - IPC socket serving
  cors.rs         - CORS with tower-http
tests/
  common/mod.rs   - Shared test router and test helpers
  common/ws_client.rs - WebSocket test client
  axum_ws.rs      - Axum WebSocket tests
  ipc.rs          - IPC tests
  ws.rs           - WebSocket tests
```

### Features

- `default = ["axum", "ws", "ipc"]`
- `axum` - HTTP serving via axum, enables `dep:axum`, `dep:mime`, `dep:opentelemetry-http`
- `pubsub` - Base pubsub framework, enables `dep:tokio-stream`, axum ws support
- `ws` - WebSocket via tokio-tungstenite (implies `pubsub`)
- `ipc` - IPC via interprocess local sockets (implies `pubsub`)

When linting, always check both `--all-features` and `--no-default-features`.

## Key Types

### `Router<S>`

The main entry point. `S` is "missing" state that must be provided via
`with_state()` before the router can serve requests.

```rust
Router::<MyState>::new()
    .route("method_name", handler_fn)
    .with_state(my_state)  // S -> () after this
```

Key methods:
- `new()` / `new_named(service_name)` - constructors
- `route(name, handler)` - register a method
- `nest(prefix, other_router)` - namespace methods under a prefix
- `merge(other_router)` - combine routers
- `with_state(state)` - provide missing state
- `fallback(handler)` - custom handler for unknown methods
- `into_axum(path)` - convert to axum router for HTTP POST
- `into_axum_with_ws(post_path, ws_path)` - HTTP + WebSocket
- `serve_pubsub(connect)` - serve over pubsub transport (Router<()> only)

Wraps `Arc<RouterInner<S>>` internally, so cloning is cheap.

### `Handler<T, S>` trait

Blanket-implemented for async functions with these signatures:
- `async fn() -> Result<T, E>` or `-> ResponsePayload<T, E>`
- `async fn(HandlerCtx) -> ...`
- `async fn(Params) -> ...`
- `async fn(HandlerCtx, Params) -> ...`
- `async fn(Params, State) -> ...`
- `async fn(HandlerCtx, Params, State) -> ...`
- `async fn(State) -> ...`
- `async fn(HandlerCtx, State) -> ...`

When `Params` type matches `State` type, use `Params<T>` / `State<S>` wrapper
structs to disambiguate.

### `HandlerCtx`

Connection context passed to handlers. Provides:
- `notify(&T)` - send pubsub notification to client
- `notifications_enabled()` - check if notifications are supported
- `spawn(future)` - spawn cancellable task (cancelled on disconnect)
- `spawn_with_ctx(|ctx| future)` - spawn with context access
- `spawn_blocking(future)` - spawn on blocking runtime
- `spawn_graceful(|cancel_token| future)` - spawn with graceful shutdown
- `span()` - get the tracing span

Handlers SHOULD use `ctx.spawn()` instead of `tokio::spawn()` for proper
connection lifecycle management.

### `ResponsePayload<T, E>`

Response enum wrapping `Result<T, ErrorPayload>`. Provides constructors for
standard JSON-RPC error codes:
- `internal_error_message(msg)`
- `internal_error_with_obj(data)`
- `method_not_found()`
- etc.

When returning `Err(e)` from a handler, `e` appears in the `data` field with
an internal error code. For custom error codes/messages, return
`ResponsePayload` directly.

### Primitive Traits

- `RpcSend` - `Serialize + Clone + Send + Sync + Unpin + 'static`
- `RpcRecv` - `DeserializeOwned + Send + Sync + Unpin + 'static`
- `RpcBorrow<'de>` - `Deserialize<'de> + Send + Sync + Unpin + 'static`
- `MethodId` - internal u64 identifier for methods

## Serving

### HTTP (axum feature)

```rust
let router = Router::new().route("method", handler).with_state(());
let axum_app = router.into_axum("/rpc");
let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
axum::serve(listener, axum_app).await?;
```

### HTTP + WebSocket (axum + pubsub features)

```rust
let router = Router::new().route("method", handler);
let axum_app = router.into_axum_with_ws("/rpc", "/ws");
```

### WebSocket standalone (ws feature)

```rust
let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 3000));
let shutdown = addr.serve(router).await?;
// shutdown drops -> server stops
```

### IPC (ipc feature)

```rust
use interprocess::local_socket::ListenerOptions;
let opts = ListenerOptions::new().name("socket_path");
let shutdown = opts.serve(router).await?;
```

## Internal Patterns

### Macros

- `tap_inner!(self, mut this => { ... })` - modify Arc<RouterInner> in place (clone-on-write)
- `map_inner!(self, inner => expr)` - transform RouterInner
- `panic_on_err!(expr)` - unwrap or panic with the error
- `impl_handler_call!(args, self(params: Type, state))` - generate handler call impl body

### Method<S> enum

```rust
enum Method<S> {
    Ready(Route),           // state already provided
    Needs(BoxedIntoRoute),  // needs state S before it can handle requests
}
```

`with_state()` converts `Needs` -> `Ready`.

### Type Erasure

Handlers are erased via `ErasedIntoRoute<S>` trait, boxed into
`BoxedIntoRoute(Box<dyn ErasedIntoRoute<S>>)`. This allows storing
heterogeneous handlers in a `BTreeMap<MethodId, Method<S>>`.

### OpenTelemetry

Spans follow OTEL semantic conventions for JSON-RPC:
- `rpc.system = "jsonrpc"`
- `rpc.jsonrpc.version = "2.0"`
- `rpc.method`, `rpc.jsonrpc.request_id`, `rpc.jsonrpc.error_code`
- `rpc.service` = router's service name (default "ajj")

### Tower Integration

`Router<()>` implements `tower::Service<HandlerArgs>` (infallible). This
enables use with any tower middleware layer.

## Testing Patterns

Test router creation helper in `tests/common/mod.rs`:
```rust
fn test_router() -> Router<()> {
    Router::<u64>::new()
        .route("hello_world", || async { Ok::<_, ()>("Hello, world!") })
        .route("add", |params: u64, state: u64| async move {
            Ok::<_, ()>(params + state)
        })
        .with_state(3u64)
}
```

Tests use `assert_rv_eq` to compare `RawValue` output against expected JSON
strings. Tests panic on failure rather than returning Result types.
