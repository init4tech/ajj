//! Basic example of using ajj with CORS.
//!
//! This example demonstrates how to set up a simple HTTP server using `axum`
//! and `tower_http` for CORS support.
//!
//! ## Inovking this example
//!
//! ```ignore
//! // Invoking with cors domains
//! cargo run --example cors -- "https://example.com,https://example.org"
//! // Invoking with wildcard
//! cargo run --example cors
//! ```

use axum::http::{HeaderValue, Method};
use eyre::{ensure, Context};
use std::{future::IntoFuture, net::SocketAddr};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

fn get_allowed(cors: &str) -> eyre::Result<AllowOrigin> {
    // Wildcard `*` means any origin is allowed.
    if cors == "*" {
        return Ok(AllowOrigin::any());
    }

    // Check if the cors string contains a wildcard
    ensure!(
        !cors.split(',').any(|o| o == "*"),
        "Wildcard '*' is not allowed in CORS domains"
    );

    // Split the cors string by commas and parse each domain into a HeaderValue
    // Then convert the Vec<HeaderValue> into AllowOrigin
    cors.split(',')
        .map(|domain| {
            // Parse each substring into a HeaderValue
            // We print parsing errors to stderr, because this is an example :)
            domain
                .parse::<HeaderValue>()
                .inspect_err(|e| eprintln!("Failed to parse domain {}: {}", domain, e))
                .wrap_err_with(|| format!("Invalid CORS domain: {}", domain))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Into::into)
}

fn make_cors(cors: &str) -> eyre::Result<CorsLayer> {
    let origins = get_allowed(cors)?;

    Ok(CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_origin(origins)
        .allow_headers(Any))
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let cors = std::env::args().nth(1).unwrap_or("*".to_string());

    // Setting up an AJJ router is easy and fun!
    let router = ajj::Router::<()>::new()
        .route("helloWorld", || async {
            tracing::info!("serving hello world");
            Ok::<_, ()>("Hello, world!")
        })
        .route("addNumbers", |(a, b): (u32, u32)| async move {
            tracing::info!("serving addNumbers");
            Ok::<_, ()>(a + b)
        })
        // Convert to an axum router
        .into_axum("/")
        // And then layer on your CORS settings
        .layer(make_cors(&cors)?);

    // Now we can serve the router on a TCP listener
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(listener, router).into_future().await?;
    Ok(())
}
