use ajj::{HandlerCtx, Router};
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
