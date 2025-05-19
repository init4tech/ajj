use axum::http::{HeaderValue, Method};
use eyre::{ensure, Context};
use std::{future::IntoFuture, net::SocketAddr};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

fn make_cors(cors: Option<&str>) -> eyre::Result<CorsLayer> {
    let origins = match cors {
        None | Some("*") => AllowOrigin::any(),
        Some(cors) => {
            ensure!(
                !cors.split(',').any(|o| o == "*"),
                "Wildcard '*' is not allowed in CORS domains"
            );

            cors.split(',')
                .map(|domain| {
                    domain
                        .parse::<HeaderValue>()
                        .wrap_err_with(|| format!("Invalid CORS domain: {}", domain))
                })
                .collect::<Result<Vec<_>, _>>()?
                .into()
        }
    };

    Ok(CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_origin(origins)
        .allow_headers(Any))
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
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
        .layer(make_cors(cors.as_deref())?);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(listener, router).into_future().await?;
    Ok(())
}
