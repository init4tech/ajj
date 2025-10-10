//! Basic example of running a JSON-RPC server over IPC using ajj
//!
//! This example demonstrates how to set up a simple IPC server using `ajj`.

use ajj::{
    pubsub::{ipc::local_socket, Connect},
    HandlerCtx, Router,
};
use tempfile::NamedTempFile;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let router = make_router();

    // Create a temporary file for the IPC socket.
    let tempfile = NamedTempFile::new()?;
    let name = to_name(tempfile.path().as_os_str()).expect("invalid name");

    println!("Serving IPC on socket: {:?}", tempfile.path());
    println!("use Ctrl-C to stop");

    // The guard keeps the server running until dropped.
    let guard = ajj::pubsub::ipc::ListenerOptions::new()
        .name(name)
        .serve(router)
        .await?;

    // Shut down on Ctrl-C
    tokio::signal::ctrl_c().await?;
    drop(guard);

    Ok(())
}

fn to_name(path: &std::ffi::OsStr) -> std::io::Result<local_socket::Name<'_>> {
    if cfg!(windows) && !path.as_encoded_bytes().starts_with(br"\\.\pipe\") {
        local_socket::ToNsName::to_ns_name::<local_socket::GenericNamespaced>(path)
    } else {
        local_socket::ToFsName::to_fs_name::<local_socket::GenericFilePath>(path)
    }
}

// Setting up an AJJ router is easy and fun!
fn make_router() -> Router<()> {
    Router::<()>::new()
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
