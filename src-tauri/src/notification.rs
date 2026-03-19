/// Push-notification helpers: Windows toast display and Axum webhook server.
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::post,
    Router,
};
use serde_json::Value;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

pub fn show_toast(app: &AppHandle, title: &str, body: &str) {
    let _ = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show();
}

pub fn get_local_ip() -> String {
    use std::net::UdpSocket;
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("8.8.8.8:80")?;
            s.local_addr()
        })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

#[derive(Clone)]
struct WebhookState {
    push_secret: String,
    app: AppHandle,
}

pub async fn start_webhook_server(port: u16, push_secret: String, app: AppHandle) {
    let state = WebhookState { push_secret, app };
    let router = Router::new()
        .route("/push/:secret", post(handle_push))
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => {
            log::info!("[notification] Webhook server listening on {addr}");
            let _ = axum::serve(listener, router).await;
        }
        Err(e) => log::error!("[notification] Failed to bind {addr}: {e}"),
    }
}

async fn handle_push(
    Path(secret): Path<String>,
    State(s): State<WebhookState>,
    body: axum::body::Bytes,
) -> StatusCode {
    if secret != s.push_secret {
        return StatusCode::NOT_FOUND;
    }
    let payload: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return StatusCode::BAD_REQUEST,
    };
    let title = payload["title"].as_str().unwrap_or("ItsyHome");
    let message = payload["message"]
        .as_str()
        .or_else(|| payload["body"].as_str())
        .or_else(|| payload["alert"].as_str())
        .unwrap_or("");
    show_toast(&s.app, title, message);
    StatusCode::OK
}
