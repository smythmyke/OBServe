use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_tungstenite::tungstenite::Message;

use crate::obs_state::{FilterInfo, InputInfo, ObsStats, SharedObsState};
use tauri::Emitter;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ObsStatus {
    pub connected: bool,
    pub obs_version: Option<String>,
    pub ws_version: Option<String>,
}

pub struct ObsConnection {
    sender: Option<mpsc::Sender<Message>>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>,
    status: ObsStatus,
    connected_flag: Arc<AtomicBool>,
}

impl ObsConnection {
    pub fn new() -> Self {
        Self {
            sender: None,
            pending: Arc::new(Mutex::new(HashMap::new())),
            status: ObsStatus {
                connected: false,
                obs_version: None,
                ws_version: None,
            },
            connected_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn status(&self) -> ObsStatus {
        ObsStatus {
            connected: self.connected_flag.load(Ordering::Relaxed),
            ..self.status.clone()
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected_flag.load(Ordering::Relaxed)
    }

    pub async fn connect(
        &mut self,
        host: &str,
        port: u16,
        password: Option<&str>,
        app_handle: tauri::AppHandle,
        obs_state: SharedObsState,
    ) -> Result<(), String> {
        if self.is_connected() {
            self.disconnect().await;
        }

        let url = format!("ws://{}:{}", host, port);
        let (ws_stream, _) = tokio_tungstenite::connect_async(&url)
            .await
            .map_err(|e| format!("Failed to connect to OBS: {}", e))?;

        let (mut write, mut read) = ws_stream.split();

        let hello: Value = loop {
            let msg = read
                .next()
                .await
                .ok_or("Connection closed before Hello")?
                .map_err(|e| format!("WebSocket error: {}", e))?;

            match &msg {
                Message::Text(text) => {
                    let text_str: &str = text.as_ref();
                    if text_str.is_empty() {
                        continue;
                    }
                    break serde_json::from_str(text_str).map_err(|e| {
                        format!(
                            "Invalid JSON from OBS: {} (raw: {})",
                            e,
                            &text_str[..text_str.len().min(200)]
                        )
                    })?;
                }
                Message::Ping(_) | Message::Pong(_) => continue,
                Message::Close(frame) => {
                    return Err(format!("OBS closed connection: {:?}", frame));
                }
                other => {
                    return Err(format!(
                        "Unexpected message type from OBS: {:?}",
                        other
                    ));
                }
            }
        };

        let op = hello["op"].as_u64().unwrap_or(0);
        if op != 0 {
            return Err(format!("Expected Hello (op 0), got op {}", op));
        }

        let hello_data = &hello["d"];
        let obs_version = hello_data["obsWebSocketVersion"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        // Event subscription bitmask:
        // General(1) | Config(2) | Scenes(4) | Inputs(8) | Filters(32) | Outputs(64) | SceneItems(128) = 239
        let event_subscriptions: u64 = 1 | 2 | 4 | 8 | 32 | 64 | 128;

        let mut identify = json!({
            "op": 1,
            "d": {
                "rpcVersion": 1,
                "eventSubscriptions": event_subscriptions
            }
        });

        if let Some(auth) = hello_data.get("authentication") {
            let pw =
                password.ok_or("OBS requires a password but none was provided")?;
            let challenge = auth["challenge"]
                .as_str()
                .ok_or("Missing auth challenge")?;
            let salt = auth["salt"].as_str().ok_or("Missing auth salt")?;
            let auth_string = generate_auth_string(pw, salt, challenge);
            identify["d"]["authentication"] = json!(auth_string);
        }

        write
            .send(Message::Text(identify.to_string().into()))
            .await
            .map_err(|e| format!("Failed to send Identify: {}", e))?;

        let identified: Value = loop {
            let msg = read
                .next()
                .await
                .ok_or("Connection closed before Identified")?
                .map_err(|e| format!("WebSocket error: {}", e))?;

            match &msg {
                Message::Text(text) => {
                    let text_str: &str = text.as_ref();
                    if text_str.is_empty() {
                        continue;
                    }
                    break serde_json::from_str(text_str).map_err(|e| {
                        format!(
                            "Invalid JSON from OBS: {} (raw: {})",
                            e,
                            &text_str[..text_str.len().min(200)]
                        )
                    })?;
                }
                Message::Ping(_) | Message::Pong(_) => continue,
                Message::Close(frame) => {
                    return Err(format!("OBS closed connection: {:?}", frame));
                }
                _ => continue,
            }
        };

        if identified["op"].as_u64().unwrap_or(0) != 2 {
            return Err("Authentication failed".to_string());
        }

        let negotiated_version = identified["d"]["negotiatedRpcVersion"]
            .as_u64()
            .unwrap_or(1);

        let (tx, mut rx) = mpsc::channel::<Message>(32);
        let pending = self.pending.clone();
        let connected_flag = self.connected_flag.clone();
        connected_flag.store(true, Ordering::Relaxed);

        let loop_state = obs_state.clone();
        let loop_app = app_handle.clone();
        let loop_connected = connected_flag.clone();
        let stats_sender = tx.clone();

        tokio::spawn(async move {
            let mut stats_interval =
                tokio::time::interval(std::time::Duration::from_secs(5));
            stats_interval.tick().await; // skip immediate first tick

            let mut prev_render_skipped: u64 = 0;
            let mut prev_output_skipped: u64 = 0;

            loop {
                tokio::select! {
                    Some(msg) = rx.recv() => {
                        if write.send(msg).await.is_err() {
                            break;
                        }
                    }
                    msg_result = read.next() => {
                        match msg_result {
                            Some(Ok(msg)) => {
                                if let Message::Text(text) = msg {
                                    if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
                                        let op = parsed["op"].as_u64().unwrap_or(0);
                                        match op {
                                            5 => {
                                                handle_event(&parsed["d"], &loop_state, &loop_app).await;
                                            }
                                            7 => {
                                                if let Some(request_id) = parsed["d"]["requestId"].as_str() {
                                                    if request_id.starts_with("__stats_") {
                                                        let (render, output) = handle_stats_response(&parsed["d"], &loop_state, &loop_app).await;
                                                        let render_delta = render.saturating_sub(prev_render_skipped);
                                                        let output_delta = output.saturating_sub(prev_output_skipped);
                                                        if prev_render_skipped > 0 || prev_output_skipped > 0 {
                                                            if render_delta >= 50 || output_delta >= 50 {
                                                                let _ = loop_app.emit("obs://frame-drop-alert", serde_json::json!({
                                                                    "renderDelta": render_delta,
                                                                    "outputDelta": output_delta,
                                                                }));
                                                            }
                                                        }
                                                        prev_render_skipped = render;
                                                        prev_output_skipped = output;
                                                    } else {
                                                        let mut pending_lock = pending.lock().await;
                                                        if let Some(sender) = pending_lock.remove(request_id) {
                                                            let _ = sender.send(parsed["d"].clone());
                                                        }
                                                    }
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                            Some(Err(_)) | None => break,
                        }
                    }
                    _ = stats_interval.tick() => {
                        let request_id = format!("__stats_{}", uuid::Uuid::new_v4());
                        let msg = json!({
                            "op": 6,
                            "d": {
                                "requestType": "GetStats",
                                "requestId": request_id,
                            }
                        });
                        // Send directly — no pending entry needed, handled by prefix check
                        let _ = stats_sender.send(Message::Text(msg.to_string().into())).await;
                    }
                }
            }

            loop_connected.store(false, Ordering::Relaxed);
            {
                let mut s = loop_state.write().await;
                s.clear();
            }
            let _ = loop_app.emit("obs://disconnected", ());
            // Clean up any pending requests
            let mut pending_lock = pending.lock().await;
            pending_lock.clear();
        });

        self.sender = Some(tx);
        self.status = ObsStatus {
            connected: true,
            obs_version: Some(obs_version),
            ws_version: Some(format!("RPC v{}", negotiated_version)),
        };

        Ok(())
    }

    pub async fn disconnect(&mut self) {
        if let Some(sender) = self.sender.take() {
            drop(sender);
        }
        self.connected_flag.store(false, Ordering::Relaxed);
        self.status = ObsStatus {
            connected: false,
            obs_version: None,
            ws_version: None,
        };
    }

    pub async fn send_request(
        &self,
        request_type: &str,
        request_data: Option<Value>,
    ) -> Result<Value, String> {
        let sender = self
            .sender
            .as_ref()
            .ok_or("Not connected to OBS")?;

        let request_id = uuid::Uuid::new_v4().to_string();

        let mut msg = json!({
            "op": 6,
            "d": {
                "requestType": request_type,
                "requestId": request_id,
            }
        });

        if let Some(data) = request_data {
            msg["d"]["requestData"] = data;
        }

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(request_id.clone(), tx);
        }

        sender
            .send(Message::Text(msg.to_string().into()))
            .await
            .map_err(|e| {
                let pending = self.pending.clone();
                let rid = request_id.clone();
                tokio::spawn(async move {
                    pending.lock().await.remove(&rid);
                });
                format!("Failed to send request: {}", e)
            })?;

        let response =
            tokio::time::timeout(std::time::Duration::from_secs(10), rx)
                .await
                .map_err(|_| "Request timed out".to_string())?
                .map_err(|_| "Response channel closed".to_string())?;

        let status = &response["requestStatus"];
        let result = status["result"].as_bool().unwrap_or(false);
        if !result {
            let code = status["code"].as_u64().unwrap_or(0);
            let comment = status["comment"].as_str().unwrap_or("Unknown error");
            return Err(format!("OBS error {}: {}", code, comment));
        }

        Ok(response
            .get("responseData")
            .cloned()
            .unwrap_or(json!({})))
    }
}

async fn handle_event(
    data: &Value,
    state: &SharedObsState,
    app: &tauri::AppHandle,
) {
    let event_type = data["eventType"].as_str().unwrap_or("");
    let event_data = &data["eventData"];

    match event_type {
        "InputVolumeChanged" => {
            let name = event_data["inputName"].as_str().unwrap_or("");
            let volume_db = event_data["inputVolumeDb"].as_f64().unwrap_or(0.0);
            let volume_mul =
                event_data["inputVolumeMul"].as_f64().unwrap_or(1.0);
            {
                let mut s = state.write().await;
                if let Some(input) = s.inputs.get_mut(name) {
                    input.volume_db = volume_db;
                    input.volume_mul = volume_mul;
                }
            }
            let _ = app.emit(
                "obs://input-volume-changed",
                json!({"inputName": name, "inputVolumeDb": volume_db, "inputVolumeMul": volume_mul}),
            );
        }
        "InputMuteStateChanged" => {
            let name = event_data["inputName"].as_str().unwrap_or("");
            let muted = event_data["inputMuted"].as_bool().unwrap_or(false);
            {
                let mut s = state.write().await;
                if let Some(input) = s.inputs.get_mut(name) {
                    input.muted = muted;
                }
            }
            let _ = app.emit(
                "obs://input-mute-changed",
                json!({"inputName": name, "inputMuted": muted}),
            );
        }
        "CurrentProgramSceneChanged" => {
            let name = event_data["sceneName"].as_str().unwrap_or("").to_string();
            {
                let mut s = state.write().await;
                s.current_scene = name.clone();
            }
            let _ = app.emit(
                "obs://current-scene-changed",
                json!({"sceneName": name}),
            );
        }
        "SceneListChanged" => {
            let scenes = event_data["scenes"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .enumerate()
                        .map(|(i, s)| {
                            crate::obs_state::SceneInfo {
                                name: s["sceneName"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string(),
                                index: i as u32,
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            {
                let mut s = state.write().await;
                s.scenes = scenes;
            }
            let state_snapshot = state.read().await.clone();
            let _ = app.emit("obs://scene-list-changed", &state_snapshot.scenes);
        }
        "InputCreated" => {
            let name = event_data["inputName"].as_str().unwrap_or("").to_string();
            let kind = event_data["inputKind"].as_str().unwrap_or("").to_string();
            let device_id = event_data["defaultInputSettings"]["device_id"]
                .as_str()
                .unwrap_or("")
                .to_string();
            {
                let mut s = state.write().await;
                s.inputs.insert(
                    name.clone(),
                    InputInfo {
                        name: name.clone(),
                        kind,
                        volume_db: 0.0,
                        volume_mul: 1.0,
                        muted: false,
                        monitor_type: String::new(),
                        filters: vec![],
                        device_id,
                        audio_balance: 0.5,
                        audio_sync_offset: 0,
                        audio_tracks: serde_json::json!({"1":true,"2":true,"3":false,"4":false,"5":false,"6":false}),
                    },
                );
            }
            let _ = app.emit("obs://input-created", json!({"inputName": name}));
        }
        "InputRemoved" => {
            let name = event_data["inputName"].as_str().unwrap_or("");
            {
                let mut s = state.write().await;
                s.inputs.remove(name);
            }
            let _ = app.emit("obs://input-removed", json!({"inputName": name}));
        }
        "InputNameChanged" => {
            let old_name = event_data["oldInputName"].as_str().unwrap_or("");
            let new_name =
                event_data["inputName"].as_str().unwrap_or("").to_string();
            {
                let mut s = state.write().await;
                if let Some(mut input) = s.inputs.remove(old_name) {
                    input.name = new_name.clone();
                    s.inputs.insert(new_name.clone(), input);
                }
            }
            let _ = app.emit(
                "obs://input-name-changed",
                json!({"oldInputName": old_name, "inputName": new_name}),
            );
        }
        "InputSettingsChanged" => {
            let name = event_data["inputName"].as_str().unwrap_or("");
            if let Some(device_id) = event_data["inputSettings"]["device_id"].as_str() {
                {
                    let mut s = state.write().await;
                    if let Some(input) = s.inputs.get_mut(name) {
                        input.device_id = device_id.to_string();
                    }
                }
            }
            let _ = app.emit(
                "obs://input-settings-changed",
                json!({"inputName": name}),
            );
        }
        "InputAudioBalanceChanged" => {
            let name = event_data["inputName"].as_str().unwrap_or("");
            let balance = event_data["inputAudioBalance"].as_f64().unwrap_or(0.5);
            {
                let mut s = state.write().await;
                if let Some(input) = s.inputs.get_mut(name) {
                    input.audio_balance = balance;
                }
            }
            let _ = app.emit(
                "obs://input-balance-changed",
                json!({"inputName": name, "inputAudioBalance": balance}),
            );
        }
        "InputAudioSyncOffsetChanged" => {
            let name = event_data["inputName"].as_str().unwrap_or("");
            let offset = event_data["inputAudioSyncOffset"].as_i64().unwrap_or(0);
            {
                let mut s = state.write().await;
                if let Some(input) = s.inputs.get_mut(name) {
                    input.audio_sync_offset = offset;
                }
            }
            let _ = app.emit(
                "obs://input-sync-offset-changed",
                json!({"inputName": name, "inputAudioSyncOffset": offset}),
            );
        }
        "InputAudioTracksChanged" => {
            let name = event_data["inputName"].as_str().unwrap_or("");
            let tracks = event_data["inputAudioTracks"].clone();
            {
                let mut s = state.write().await;
                if let Some(input) = s.inputs.get_mut(name) {
                    input.audio_tracks = tracks.clone();
                }
            }
            let _ = app.emit(
                "obs://input-tracks-changed",
                json!({"inputName": name, "inputAudioTracks": tracks}),
            );
        }
        "InputAudioMonitorTypeChanged" => {
            let name = event_data["inputName"].as_str().unwrap_or("");
            let monitor_type = event_data["monitorType"].as_str().unwrap_or("").to_string();
            {
                let mut s = state.write().await;
                if let Some(input) = s.inputs.get_mut(name) {
                    input.monitor_type = monitor_type.clone();
                }
            }
            let _ = app.emit(
                "obs://monitor-type-changed",
                json!({"inputName": name, "monitorType": monitor_type}),
            );
        }
        "SourceFilterCreated" => {
            let source = event_data["sourceName"].as_str().unwrap_or("").to_string();
            let filter_name = event_data["filterName"].as_str().unwrap_or("").to_string();
            let filter_kind = event_data["filterKind"].as_str().unwrap_or("").to_string();
            let filter_settings = event_data["filterSettings"].clone();
            {
                let mut s = state.write().await;
                if let Some(input) = s.inputs.get_mut(&source) {
                    let idx = event_data["filterIndex"].as_u64().unwrap_or(input.filters.len() as u64) as usize;
                    let filter = FilterInfo {
                        name: filter_name,
                        kind: filter_kind,
                        enabled: true,
                        settings: filter_settings,
                    };
                    if idx <= input.filters.len() {
                        input.filters.insert(idx, filter);
                    } else {
                        input.filters.push(filter);
                    }
                }
            }
            let _ = app.emit("obs://filters-changed", json!({"sourceName": source}));
        }
        "SourceFilterRemoved" => {
            let source = event_data["sourceName"].as_str().unwrap_or("").to_string();
            let filter_name = event_data["filterName"].as_str().unwrap_or("");
            {
                let mut s = state.write().await;
                if let Some(input) = s.inputs.get_mut(&source) {
                    input.filters.retain(|f| f.name != filter_name);
                }
            }
            let _ = app.emit("obs://filters-changed", json!({"sourceName": source}));
        }
        "SourceFilterEnableStateChanged" => {
            let source = event_data["sourceName"].as_str().unwrap_or("").to_string();
            let filter_name = event_data["filterName"].as_str().unwrap_or("");
            let enabled = event_data["filterEnabled"].as_bool().unwrap_or(true);
            {
                let mut s = state.write().await;
                if let Some(input) = s.inputs.get_mut(&source) {
                    if let Some(f) = input.filters.iter_mut().find(|f| f.name == filter_name) {
                        f.enabled = enabled;
                    }
                }
            }
            let _ = app.emit("obs://filters-changed", json!({"sourceName": source}));
        }
        "SourceFilterNameChanged" => {
            let source = event_data["sourceName"].as_str().unwrap_or("").to_string();
            let old_name = event_data["filterName"].as_str().unwrap_or("");
            let new_name = event_data["newFilterName"].as_str().unwrap_or("").to_string();
            {
                let mut s = state.write().await;
                if let Some(input) = s.inputs.get_mut(&source) {
                    if let Some(f) = input.filters.iter_mut().find(|f| f.name == old_name) {
                        f.name = new_name;
                    }
                }
            }
            let _ = app.emit("obs://filters-changed", json!({"sourceName": source}));
        }
        "SourceFilterListReindexed" => {
            let source = event_data["sourceName"].as_str().unwrap_or("").to_string();
            // Reorder filters based on the provided list
            if let Some(filters_arr) = event_data["filters"].as_array() {
                let mut s = state.write().await;
                if let Some(input) = s.inputs.get_mut(&source) {
                    let mut reordered = Vec::new();
                    for item in filters_arr {
                        let name = item["filterName"].as_str().unwrap_or("");
                        if let Some(f) = input.filters.iter().find(|f| f.name == name) {
                            reordered.push(f.clone());
                        }
                    }
                    if !reordered.is_empty() {
                        input.filters = reordered;
                    }
                }
            }
            let _ = app.emit("obs://filters-changed", json!({"sourceName": source}));
        }
        "SourceFilterSettingsChanged" => {
            let source = event_data["sourceName"].as_str().unwrap_or("").to_string();
            let filter_name = event_data["filterName"].as_str().unwrap_or("");
            let new_settings = &event_data["filterSettings"];
            {
                let mut s = state.write().await;
                if let Some(input) = s.inputs.get_mut(&source) {
                    if let Some(f) = input.filters.iter_mut().find(|f| f.name == filter_name) {
                        if let (Some(existing), Some(incoming)) = (f.settings.as_object_mut(), new_settings.as_object()) {
                            for (k, v) in incoming {
                                existing.insert(k.clone(), v.clone());
                            }
                        }
                    }
                }
            }
            let _ = app.emit("obs://filters-changed", json!({"sourceName": source}));
        }
        "StreamStateChanged" => {
            let active =
                event_data["outputActive"].as_bool().unwrap_or(false);
            {
                let mut s = state.write().await;
                s.stream_status.active = active;
            }
            let _ = app.emit(
                "obs://stream-state-changed",
                json!({"outputActive": active}),
            );
        }
        "RecordStateChanged" => {
            let active =
                event_data["outputActive"].as_bool().unwrap_or(false);
            {
                let mut s = state.write().await;
                s.record_status.active = active;
                if !active {
                    s.record_status.paused = false;
                }
            }
            let _ = app.emit(
                "obs://record-state-changed",
                json!({"outputActive": active}),
            );
        }
        _ => {}
    }
}

async fn handle_stats_response(
    data: &Value,
    state: &SharedObsState,
    app: &tauri::AppHandle,
) -> (u64, u64) {
    if let Some(resp) = data.get("responseData") {
        let stats = ObsStats {
            active_fps: resp["activeFps"].as_f64().unwrap_or(0.0),
            cpu_usage: resp["cpuUsage"].as_f64().unwrap_or(0.0),
            memory_usage: resp["memoryUsage"].as_f64().unwrap_or(0.0),
            render_skipped_frames: resp["renderSkippedFrames"]
                .as_u64()
                .unwrap_or(0),
            output_skipped_frames: resp["outputSkippedFrames"]
                .as_u64()
                .unwrap_or(0),
        };
        let render = stats.render_skipped_frames;
        let output = stats.output_skipped_frames;
        {
            let mut s = state.write().await;
            s.stats = stats.clone();
        }
        let _ = app.emit("obs://stats-updated", &stats);
        (render, output)
    } else {
        (0, 0)
    }
}

fn generate_auth_string(
    password: &str,
    salt: &str,
    challenge: &str,
) -> String {
    let secret_hash =
        Sha256::digest(format!("{}{}", password, salt).as_bytes());
    let secret =
        base64::engine::general_purpose::STANDARD.encode(secret_hash);
    let auth_hash =
        Sha256::digest(format!("{}{}", secret, challenge).as_bytes());
    base64::engine::general_purpose::STANDARD.encode(auth_hash)
}
