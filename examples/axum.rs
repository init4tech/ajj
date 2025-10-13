use ajj::HandlerCtx;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let router = make_router();

    // Serve via `POST` on `/` and Websockets on `/ws`
    let axum = router.clone().into_axum_with_ws("/", "/ws");

    // Now we can serve the router on a TCP listener
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    println!("Listening for POST on {}/", listener.local_addr()?);
    println!("Listening for WS on {}/ws", listener.local_addr()?);

    println!("use Ctrl-C to stop");
    axum::serve(listener, axum).await.map_err(Into::into)
}

fn make_router() -> ajj::Router<()> {
    ajj::Router::<()>::new()
        .route("helloWorld", || async {
            tracing::info!("serving hello world");
            Ok::<_, ()>("Hello, world!")
        })
        .route("addNumbers", |(a, b): (u32, u32)| async move {
            tracing::info!("serving addNumbers");
            Ok::<_, ()>(a + b)
        })
        .route("notify", |ctx: HandlerCtx| async move {
            // Check if notifications are enabled for the connection.
            if !ctx.notifications_enabled() {
                // This error will appear in the ResponsePayload's `data` field.
                return Err("notifications are disabled");
            }

            let req_id = 15u8;

            // Spawn a task to send the notification after a short delay.
            ctx.spawn_with_ctx(|ctx| async move {
                // something expensive goes here
                let result = 100_000_000;
                let _ = ctx
                    .notify(&serde_json::json!({
                      "req_id": req_id,
                      "result": result,
                    }))
                    .await;
            });

            // Return the request ID immediately.
            Ok(req_id)
        })
}
