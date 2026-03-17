/// HAClient — async actor pattern over a WebSocket connection to Home Assistant.
/// Faithful Rust/tokio port of `client.ts`.
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time::sleep;
use tokio_tungstenite::{
    connect_async,
    tungstenite::protocol::Message,
};

use super::models::{HaArea, HaConfig, HaDevice, HaEntityEntry, HaNotification, HaState};

// ─── Reconnect schedule ───────────────────────────────────────────────────────

const RECONNECT_DELAYS_SECS: &[u64] = &[1, 2, 4, 8, 16, 30];
const PING_INTERVAL_SECS: u64 = 30;
const PING_TIMEOUT_SECS: u64 = 10;

// ─── Command enum ─────────────────────────────────────────────────────────────

/// Commands sent to the HA actor task.
pub enum HaCommand {
    /// Generic WebSocket command (get_states, etc.).
    WsCommand {
        type_: String,
        extra: Option<Value>,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    /// Convenience wrapper for `call_service`.
    CallService {
        domain: String,
        service: String,
        entity_id: String,
        data: Option<Value>,
        reply: oneshot::Sender<Result<(), String>>,
    },
    /// HTTP snapshot — uses reqwest, not WS.
    GetCameraSnapshot {
        entity_id: String,
        ha_url: String,
        token: String,
        reply: oneshot::Sender<Option<String>>,
    },
    Disconnect,
}

// ─── Public handle ────────────────────────────────────────────────────────────

/// Cheap-clone handle to the background HA actor task.
#[derive(Clone)]
pub struct HaClientHandle {
    tx: mpsc::Sender<HaCommand>,
}

impl HaClientHandle {
    pub async fn get_states(&self) -> Result<Vec<HaState>, String> {
        let v = self.ws_cmd("get_states", None).await?;
        serde_json::from_value(v).map_err(|e| e.to_string())
    }

    pub async fn get_areas(&self) -> Result<Vec<HaArea>, String> {
        let v = self.ws_cmd("config/area_registry/list", None).await?;
        serde_json::from_value(v).map_err(|e| e.to_string())
    }

    pub async fn get_devices(&self) -> Result<Vec<HaDevice>, String> {
        let v = self.ws_cmd("config/device_registry/list", None).await?;
        serde_json::from_value(v).map_err(|e| e.to_string())
    }

    pub async fn get_entities(&self) -> Result<Vec<HaEntityEntry>, String> {
        let v = self.ws_cmd("config/entity_registry/list", None).await?;
        serde_json::from_value(v).map_err(|e| e.to_string())
    }

    pub async fn get_config(&self) -> Result<HaConfig, String> {
        let v = self.ws_cmd("get_config", None).await?;
        serde_json::from_value(v).map_err(|e| e.to_string())
    }

    pub async fn get_notifications(&self) -> Result<Vec<HaNotification>, String> {
        let v = self.ws_cmd("persistent_notification/get", None).await?;
        serde_json::from_value(v).map_err(|e| e.to_string())
    }

    pub async fn call_service(
        &self,
        domain: String,
        service: String,
        entity_id: String,
        data: Option<Value>,
    ) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(HaCommand::CallService {
                domain,
                service,
                entity_id,
                data,
                reply: tx,
            })
            .await
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())?
    }

    pub async fn get_camera_snapshot(
        &self,
        entity_id: String,
        ha_url: String,
        token: String,
    ) -> Option<String> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(HaCommand::GetCameraSnapshot {
                entity_id,
                ha_url,
                token,
                reply: tx,
            })
            .await
            .ok()?;
        rx.await.ok().flatten()
    }

    pub async fn disconnect(&self) {
        let _ = self.tx.send(HaCommand::Disconnect).await;
    }

    async fn ws_cmd(&self, type_: &str, extra: Option<Value>) -> Result<Value, String> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(HaCommand::WsCommand {
                type_: type_.to_owned(),
                extra,
                reply: tx,
            })
            .await
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())?
    }
}

// ─── Actor task ───────────────────────────────────────────────────────────────

/// Spawn the long-running HA actor.  Returns a handle the rest of the app uses.
pub fn spawn_ha_task(url: String, token: String, app_handle: AppHandle) -> HaClientHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel::<HaCommand>(64);

    let cmd_tx_clone = cmd_tx.clone();
    tokio::spawn(async move {
        run_actor(url, token, app_handle, cmd_rx, cmd_tx_clone).await;
    });

    HaClientHandle { tx: cmd_tx }
}

// ─── One-shot connection test ─────────────────────────────────────────────────

/// Open a temporary WS, authenticate, fetch states, close.  10 s timeout.
pub async fn test_connection(
    url: String,
    token: String,
) -> crate::types::TestConnectionResult {
    use tokio::time::timeout;

    let ws_url = make_ws_url(&url);
    let result = timeout(Duration::from_secs(10), async move {
        let ws_stream = match connect_async(&ws_url).await {
            Ok((ws, _)) => ws,
            Err(e) => {
                return crate::types::TestConnectionResult {
                    ok: false,
                    entity_count: None,
                    error: Some(e.to_string()),
                };
            }
        };

        let (mut write, mut read) = ws_stream.split();
        let mut msg_id: u32 = 1;
        let mut authenticated = false;

        while let Some(Ok(msg)) = read.next().await {
            if let Message::Text(text) = msg {
                let v: Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                match v["type"].as_str() {
                    Some("auth_required") => {
                        let auth = json!({ "type": "auth", "access_token": token });
                        let _ = write.send(Message::Text(auth.to_string())).await;
                    }
                    Some("auth_ok") => {
                        authenticated = true;
                        let cmd = json!({ "type": "get_states", "id": msg_id });
                        msg_id += 1;
                        let _ = write.send(Message::Text(cmd.to_string())).await;
                    }
                    Some("auth_invalid") => {
                        return crate::types::TestConnectionResult {
                            ok: false,
                            entity_count: None,
                            error: Some("Invalid token".to_owned()),
                        };
                    }
                    Some("result") if authenticated => {
                        let _ = write.close().await;
                        if v["success"].as_bool().unwrap_or(false) {
                            let count =
                                v["result"].as_array().map(|a| a.len() as u32);
                            return crate::types::TestConnectionResult {
                                ok: true,
                                entity_count: count,
                                error: None,
                            };
                        } else {
                            let err = v["error"]["message"]
                                .as_str()
                                .unwrap_or("Unknown error")
                                .to_owned();
                            return crate::types::TestConnectionResult {
                                ok: false,
                                entity_count: None,
                                error: Some(err),
                            };
                        }
                    }
                    _ => {}
                }
            }
        }

        crate::types::TestConnectionResult {
            ok: false,
            entity_count: None,
            error: Some("Connection closed unexpectedly".to_owned()),
        }
    })
    .await;

    result.unwrap_or(crate::types::TestConnectionResult {
        ok: false,
        entity_count: None,
        error: Some("Timeout".to_owned()),
    })
}

// ─── Camera snapshot via HTTP ─────────────────────────────────────────────────

pub async fn fetch_camera_snapshot(
    entity_id: &str,
    ha_url: &str,
    token: &str,
) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .ok()?;

    let url = format!("{}/api/camera_proxy/{}", ha_url.trim_end_matches('/'), entity_id);
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let bytes = resp.bytes().await.ok()?;
    let b64 = BASE64.encode(&bytes);
    Some(format!("data:image/jpeg;base64,{}", b64))
}

// ─── Internal actor loop ──────────────────────────────────────────────────────

async fn run_actor(
    url: String,
    token: String,
    app_handle: AppHandle,
    mut cmd_rx: mpsc::Receiver<HaCommand>,
    _cmd_tx: mpsc::Sender<HaCommand>,
) {
    let mut reconnect_attempt: usize = 0;

    loop {
        emit_status(&app_handle, "connecting");
        let ws_url = make_ws_url(&url);

        let ws_stream = match connect_async(&ws_url).await {
            Ok((ws, _)) => ws,
            Err(e) => {
                log::warn!("[HA] connect failed: {e}");
                emit_status(&app_handle, "disconnected");
                let delay = reconnect_delay(reconnect_attempt);
                reconnect_attempt += 1;
                tokio::select! {
                    _ = sleep(Duration::from_secs(delay)) => continue,
                    cmd = cmd_rx.recv() => {
                        if matches!(cmd, None | Some(HaCommand::Disconnect)) { return; }
                    }
                }
                continue;
            }
        };

        let (write, read) = ws_stream.split();
        // Wrap in Arc<Mutex> so we can drive the sink from multiple branches
        let write = Arc::new(Mutex::new(write));

        let mut pending: HashMap<u32, oneshot::Sender<Result<Value, String>>> = HashMap::new();
        let mut msg_id: u32 = 1;
        let mut authenticated = false;

        // Timers
        let mut ping_interval =
            tokio::time::interval(Duration::from_secs(PING_INTERVAL_SECS));
        ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // When Some, a ping is in-flight and we're waiting for a pong.
        let ping_timeout: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>> =
            Arc::new(Mutex::new(None));

        // State-changed debounce
        let debounce: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>> =
            Arc::new(Mutex::new(None));

        let app_clone = app_handle.clone();
        let write_clone = write.clone();
        let ping_timeout_clone = ping_timeout.clone();
        let debounce_clone = debounce.clone();

        // Pin the read side so it implements Unpin for select!
        let mut read = read;

        'session: loop {
            tokio::select! {
                // ── Inbound WebSocket messages ────────────────────────────
                maybe_msg = read.next() => {
                    let msg = match maybe_msg {
                        Some(Ok(m)) => m,
                        _ => {
                            log::info!("[HA] WebSocket closed");
                            break 'session;
                        }
                    };

                    let text = match msg {
                        Message::Text(t) => t,
                        Message::Close(_) => break 'session,
                        _ => continue,
                    };

                    let v: Value = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    match v["type"].as_str() {
                        Some("auth_required") => {
                            let auth = json!({
                                "type": "auth",
                                "access_token": token
                            });
                            let _ = write.lock().await
                                .send(Message::Text(auth.to_string()))
                                .await;
                        }

                        Some("auth_ok") => {
                            reconnect_attempt = 0;
                            authenticated = true;
                            emit_status(&app_clone, "connected");

                            // Subscribe to state_changed
                            let sub_id = msg_id;
                            msg_id += 1;
                            let sub = json!({
                                "type": "subscribe_events",
                                "event_type": "state_changed",
                                "id": sub_id
                            });
                            let _ = write.lock().await
                                .send(Message::Text(sub.to_string()))
                                .await;

                            // Trigger initial data refresh
                            let app2 = app_clone.clone();
                            tokio::spawn(async move {
                                crate::refresh::trigger_refresh(&app2).await;
                            });
                        }

                        Some("auth_invalid") => {
                            emit_status(&app_clone, "error");
                            return; // permanent — do not reconnect
                        }

                        Some("result") => {
                            let id = v["id"].as_u64().unwrap_or(0) as u32;
                            if let Some(tx) = pending.remove(&id) {
                                if v["success"].as_bool().unwrap_or(false) {
                                    let _ = tx.send(Ok(v["result"].clone()));
                                } else {
                                    let err = v["error"]["message"]
                                        .as_str()
                                        .unwrap_or("HA error")
                                        .to_owned();
                                    let _ = tx.send(Err(err));
                                }
                            }
                        }

                        Some("pong") => {
                            // Cancel ping timeout
                            if let Some(handle) = ping_timeout.lock().await.take() {
                                handle.abort();
                            }
                        }

                        Some("event") => {
                            let et = v["event"]["event_type"].as_str().unwrap_or("");
                            if et == "state_changed" && authenticated {
                                // Debounce 300 ms
                                let app2 = app_clone.clone();
                                let mut guard = debounce.lock().await;
                                if let Some(h) = guard.take() {
                                    h.abort();
                                }
                                *guard = Some(tokio::spawn(async move {
                                    sleep(Duration::from_millis(300)).await;
                                    crate::refresh::trigger_refresh(&app2).await;
                                }));
                            }
                        }

                        _ => {}
                    }
                }

                // ── Outbound commands ─────────────────────────────────────
                maybe_cmd = cmd_rx.recv() => {
                    let cmd = match maybe_cmd {
                        Some(c) => c,
                        None => break 'session,
                    };

                    match cmd {
                        HaCommand::Disconnect => {
                            let _ = write.lock().await.close().await;
                            emit_status(&app_clone, "disconnected");
                            return;
                        }

                        HaCommand::WsCommand { type_, extra, reply } => {
                            if !authenticated {
                                let _ = reply.send(Err("Not authenticated".to_owned()));
                                continue;
                            }
                            let id = msg_id;
                            msg_id += 1;
                            let mut payload = json!({ "type": type_, "id": id });
                            if let Some(extra) = extra {
                                if let (Some(obj), Some(ext)) =
                                    (payload.as_object_mut(), extra.as_object())
                                {
                                    for (k, v) in ext {
                                        obj.insert(k.clone(), v.clone());
                                    }
                                }
                            }
                            pending.insert(id, reply);
                            let _ = write.lock().await
                                .send(Message::Text(payload.to_string()))
                                .await;
                        }

                        HaCommand::CallService {
                            domain,
                            service,
                            entity_id,
                            data,
                            reply,
                        } => {
                            if !authenticated {
                                let _ = reply.send(Err("Not authenticated".to_owned()));
                                continue;
                            }
                            let id = msg_id;
                            msg_id += 1;
                            let payload = json!({
                                "type": "call_service",
                                "id": id,
                                "domain": domain,
                                "service": service,
                                "target": { "entity_id": entity_id },
                                "service_data": data.unwrap_or(json!({}))
                            });
                            // Convert the typed reply to a plain WS reply
                            let (inner_tx, inner_rx) = oneshot::channel::<Result<Value, String>>();
                            pending.insert(id, inner_tx);
                            let _ = write.lock().await
                                .send(Message::Text(payload.to_string()))
                                .await;
                            // Bridge inner result to caller
                            tokio::spawn(async move {
                                let result = inner_rx
                                    .await
                                    .unwrap_or(Err("Channel dropped".to_owned()));
                                let _ = reply.send(result.map(|_| ()));
                            });
                        }

                        HaCommand::GetCameraSnapshot {
                            entity_id,
                            ha_url,
                            token: snap_token,
                            reply,
                        } => {
                            tokio::spawn(async move {
                                let r =
                                    fetch_camera_snapshot(&entity_id, &ha_url, &snap_token).await;
                                let _ = reply.send(r);
                            });
                        }
                    }
                }

                // ── Ping heartbeat ────────────────────────────────────────
                _ = ping_interval.tick() => {
                    if !authenticated {
                        continue;
                    }
                    let id = msg_id;
                    msg_id += 1;
                    let ping = json!({ "type": "ping", "id": id });
                    let _ = write_clone.lock().await
                        .send(Message::Text(ping.to_string()))
                        .await;

                    // Start ping-timeout watchdog
                    let write2 = write_clone.clone();
                    let pt = ping_timeout_clone.clone();
                    let handle = tokio::spawn(async move {
                        sleep(Duration::from_secs(PING_TIMEOUT_SECS)).await;
                        log::warn!("[HA] Ping timeout — closing socket");
                        let _ = write2.lock().await.close().await;
                    });
                    let mut guard = pt.lock().await;
                    if let Some(old) = guard.replace(handle) {
                        old.abort();
                    }
                }
            }
        }

        // Session ended — clean up in-flight requests
        for (_, tx) in pending.drain() {
            let _ = tx.send(Err("Disconnected".to_owned()));
        }
        if let Some(h) = debounce_clone.lock().await.take() {
            h.abort();
        }
        if let Some(h) = ping_timeout_clone.lock().await.take() {
            h.abort();
        }

        emit_status(&app_handle, "disconnected");
        let delay = reconnect_delay(reconnect_attempt);
        reconnect_attempt += 1;

        tokio::select! {
            _ = sleep(Duration::from_secs(delay)) => {}
            cmd = cmd_rx.recv() => {
                if matches!(cmd, None | Some(HaCommand::Disconnect)) { return; }
            }
        }
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn make_ws_url(http_url: &str) -> String {
    let base = http_url.trim_end_matches('/');
    let replaced = if base.starts_with("https://") {
        base.replacen("https://", "wss://", 1)
    } else {
        base.replacen("http://", "ws://", 1)
    };
    format!("{}/api/websocket", replaced)
}

fn reconnect_delay(attempt: usize) -> u64 {
    RECONNECT_DELAYS_SECS[attempt.min(RECONNECT_DELAYS_SECS.len() - 1)]
}

fn emit_status(app: &AppHandle, status: &str) {
    let _ = app.emit("ha:statusChange", status);
}
