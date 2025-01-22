use ajj::{HandlerCtx, Router};
use serde_json::Value;
use tokio::time;

/// Instantiate a router for testing.
pub fn test_router() -> ajj::Router<()> {
    Router::<()>::new()
        .route("ping", || async move { Ok::<_, ()>("pong") })
        .route(
            "double",
            |params: usize| async move { Ok::<_, ()>(params * 2) },
        )
        .route("notify", |ctx: HandlerCtx| async move {
            tokio::task::spawn(async move {
                time::sleep(time::Duration::from_millis(100)).await;

                let _ = ctx
                    .notify(&serde_json::json!({
                        "method": "notify",
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

    fn last_id(&self) -> usize;

    async fn send_raw<S: serde::Serialize>(&mut self, msg: &S);

    async fn recv<D: serde::de::DeserializeOwned>(&mut self) -> D;

    async fn send<S: serde::Serialize>(&mut self, method: &str, params: &S) {
        let id = self.next_id();
        self.send_raw(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))
        .await;
    }
}

async fn test_missing_id<T: TestClient>(client: &mut T) {
    client
        .send_raw(&serde_json::json!(
            {"jsonrpc": "2.0", "method": "ping"}
        ))
        .await;

    let next: Value = client.recv().await;
    assert_eq!(
        next,
        serde_json::json!({
            "jsonrpc": "2.0",
            "result": "pong",
            "id": null,
        })
    );
}

/// basic tests of the test router
pub async fn basic_tests<T: TestClient>(mut client: T) {
    client.send("ping", &()).await;

    let next: Value = client.recv().await;
    assert_eq!(
        next,
        serde_json::json!({"id": client.last_id(), "jsonrpc": "2.0", "result": "pong"})
    );

    client.send("double", &5).await;
    let next: Value = client.recv().await;
    assert_eq!(
        next,
        serde_json::json!({"id": client.last_id(), "jsonrpc": "2.0", "result": 10})
    );

    client.send("notify", &()).await;

    let now = std::time::Instant::now();

    let next: Value = client.recv().await;
    assert_eq!(
        next,
        serde_json::json!({"id": client.last_id(), "jsonrpc": "2.0", "result": null})
    );

    let next: Value = client.recv().await;
    assert!(now.elapsed().as_millis() >= 100);
    assert_eq!(
        next,
        serde_json::json!({"method": "notify", "result": "notified"})
    );

    test_missing_id(&mut client).await;
}
