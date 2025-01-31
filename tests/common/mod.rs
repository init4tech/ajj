use std::time::Duration;

use ajj::{HandlerCtx, Router};
use serde_json::{json, Value};
use tokio::time::{self, timeout};

/// Instantiate a router for testing.
pub fn test_router() -> ajj::Router<()> {
    Router::<()>::new()
        .route("ping", || async move { Ok::<_, ()>("pong") })
        .route(
            "double",
            |params: usize| async move { Ok::<_, ()>(params * 2) },
        )
        .route("call_me_later", |ctx: HandlerCtx| async move {
            ctx.spawn_with_ctx(|ctx| async move {
                time::sleep(time::Duration::from_millis(100)).await;

                let _ = ctx
                    .notify(&json!({
                        "method": "call_me_later",
                        "result": "notified"
                    }))
                    .await;
            });

            Ok::<_, ()>(())
        })
}

/// Test clients
pub trait TestClient {
    fn next_id(&mut self) -> usize;

    async fn send_raw<S: serde::Serialize>(&mut self, msg: &S);

    async fn recv<D: serde::de::DeserializeOwned>(&mut self) -> D;

    async fn send<S: serde::Serialize>(&mut self, method: &str, params: &S) -> usize {
        let id = self.next_id();
        self.send_raw(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))
        .await;
        id
    }
}

// SPEC: notifications receive no response.
async fn test_notification<T: TestClient>(client: &mut T) {
    client
        .send_raw(&json!(
            {"jsonrpc": "2.0", "method": "ping"}
        ))
        .await;

    let next: Result<Value, _> = timeout(Duration::from_millis(100), client.recv()).await;

    // The notification should not have a response
    assert!(next.is_err());
}

async fn test_call_me_later<T: TestClient>(client: &mut T) {
    let id = client.send("call_me_later", &()).await;

    let now = std::time::Instant::now();

    let next: Value = client.recv().await;
    assert_eq!(
        next,
        serde_json::json!({"id": id, "jsonrpc": "2.0", "result": null})
    );

    let next: Value = client.recv().await;
    assert!(now.elapsed().as_millis() >= 100);
    assert_eq!(
        next,
        serde_json::json!({"method": "call_me_later", "result": "notified"})
    );
}

async fn test_double<T: TestClient>(client: &mut T) {
    let id = client.send("double", &5).await;
    let next: Value = client.recv().await;
    assert_eq!(
        next,
        serde_json::json!({"id": id, "jsonrpc": "2.0", "result": 10})
    );
}

async fn test_ping<T: TestClient>(client: &mut T) {
    let id = client.send("ping", &()).await;

    let next: Value = client.recv().await;
    assert_eq!(
        next,
        serde_json::json!({"id": id, "jsonrpc": "2.0", "result": "pong"})
    );
}

/// basic tests of the test router
pub async fn basic_tests<T: TestClient>(client: &mut T) {
    test_ping(client).await;

    test_double(client).await;

    test_call_me_later(client).await;

    test_notification(client).await;
}

/// Test when a full batch request fails to parse
async fn batch_parse_fail<T: TestClient>(client: &mut T) {
    client.send_raw(&json!(r#""invalid request""#)).await;

    let next: Value = client.recv().await;
    assert_eq!(
        next,
        serde_json::json!({
            "id": null,
            "jsonrpc": "2.0",
            "error": {
                "code": -32700,
                "message": "Parse error"
            }
        })
    );
}

// Test when a single request fails to parse
async fn batch_single_req_parse_fail<T: TestClient>(client: &mut T) {
    client
        .send_raw(&json!([
            {"jsonrpc": "2.0", "method": "ping", "id": 1},
            {"jsonrpc": "2.0", "method": "double", "params": 5, "id": 2},
            {"jsonrpc": "2.0", "method": "ping", "id": 4},
            "invalid request"

        ]))
        .await;

    let next: Value = client.recv().await;
    let arr = next.as_array().unwrap();

    assert_eq!(
        arr[0],
        serde_json::json!({
            "id": null,
            "jsonrpc": "2.0",
            "error": {
                "code": -32700,
                "message": "Parse error"
            }
        })
    );

    assert_eq!(arr.len(), 4);
    assert!(arr.contains(&serde_json::json!({"id": 1, "jsonrpc": "2.0", "result": "pong"})));
    assert!(arr.contains(&serde_json::json!({"id": 2, "jsonrpc": "2.0", "result": 10})));
    assert!(arr.contains(&serde_json::json!({"id": 4, "jsonrpc": "2.0", "result": "pong"})));
    assert!(arr.contains(&serde_json::json!({
        "id": null,
        "jsonrpc": "2.0",
        "error": {
            "code": -32700,
            "message": "Parse error"
        }
    })));
}

// SPEC: notifications receive no response.
async fn batch_contains_notification<T: TestClient>(client: &mut T) {
    let batch = json!([
        {"jsonrpc": "2.0", "method": "ping"},
        {"jsonrpc": "2.0", "method": "double", "params": 5, "id": 2},
        {"jsonrpc": "2.0", "method": "ping", "id": 4},
    ]);

    client.send_raw(&batch).await;
    let next: Value = client.recv().await;

    let arr = next.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert!(arr.contains(&serde_json::json!({"id": 2, "jsonrpc": "2.0", "result": 10})));
    assert!(arr.contains(&serde_json::json!({"id": 4, "jsonrpc": "2.0", "result": "pong"})));
}

// SPEC: batches that contain only notifications receive no response.
async fn batch_contains_only_notifications<T: TestClient>(client: &mut T) {
    let batch = json!([
        {"jsonrpc": "2.0", "method": "ping"},
        {"jsonrpc": "2.0", "method": "ping"},
        {"jsonrpc": "2.0", "method": "ping"},
    ]);

    client.send_raw(&batch).await;
    let res = timeout(Duration::from_millis(200), client.recv::<Value>()).await;

    assert!(res.is_err());
}

/// Test of batch requests
pub async fn batch_tests<T: TestClient>(client: &mut T) {
    batch_parse_fail(client).await;

    batch_single_req_parse_fail(client).await;

    batch_contains_notification(client).await;

    batch_contains_only_notifications(client).await;
}
