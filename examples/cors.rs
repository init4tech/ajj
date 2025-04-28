use axum::http::{HeaderValue, Method};
use std::{future::IntoFuture, net::SocketAddr};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

fn make_cors(cors: Option<&str>) -> tower_http::cors::CorsLayer {
    let cors = cors
        .unwrap_or("*")
        .parse::<HeaderValue>()
        .map(Into::<AllowOrigin>::into)
        .unwrap_or_else(|_| AllowOrigin::any());

    CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_origin(cors)
        .allow_headers(Any)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cors = std::env::args().nth(1);
    let port = std::env::args()
        .nth(2)
        .and_then(|src| src.parse().ok())
        .unwrap_or(8080u16);

    let router = ajj::Router::<()>::new()
        .route("helloWorld", || async {
            tracing::info!("serving hello world");
            Ok::<_, ()>("Hello, world!")
        })
        .route("addNumbers", |(a, b): (u32, u32)| async move {
            tracing::info!("serving addNumbers");
            Ok::<_, ()>(a + b)
        })
        .into_axum("/")
        .layer(make_cors(cors.as_deref()));

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(listener, router).into_future().await?;
    Ok(())
}
