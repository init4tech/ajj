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
        .route("return_state", |s: u8| async move { Ok::<_, ()>(s) })
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
    async fn send<S: serde::Serialize>(&mut self, method: &str, params: &S);
    async fn recv<D: serde::de::DeserializeOwned>(&mut self) -> D;
}

/// basic tests of the test router
pub async fn basic_tests<T: TestClient>(mut client: T) {
    client.send("ping", &()).await;

    let next: Value = client.recv().await;
    assert_eq!(
        next,
        serde_json::json!({"id": 0, "jsonrpc": "2.0", "result": "pong"})
    );
}
