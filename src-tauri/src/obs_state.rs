use crate::obs_websocket::ObsConnection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub type SharedObsState = Arc<RwLock<ObsState>>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ObsState {
    pub scenes: Vec<SceneInfo>,
    pub current_scene: String,
    pub inputs: HashMap<String, InputInfo>,
    pub stream_status: StreamRecordStatus,
    pub record_status: StreamRecordStatus,
    pub stats: ObsStats,
    pub video_settings: VideoSettings,
    pub stream_service: StreamServiceSettings,
    pub record_settings: RecordSettings,
    pub special_inputs: SpecialInputs,
    pub scene_items: HashMap<String, Vec<SceneItemInfo>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneInfo {
    pub name: String,
    pub index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneItemInfo {
    pub source_name: String,
    pub source_kind: String,
    pub scene_item_id: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputInfo {
    pub name: String,
    pub kind: String,
    pub volume_db: f64,
    pub volume_mul: f64,
    pub muted: bool,
    pub monitor_type: String,
    pub filters: Vec<FilterInfo>,
    pub device_id: String,
    pub audio_balance: f64,
    pub audio_sync_offset: i64,
    pub audio_tracks: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SpecialInputs {
    pub desktop1: String,
    pub desktop2: String,
    pub mic1: String,
    pub mic2: String,
    pub mic3: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterInfo {
    pub name: String,
    pub kind: String,
    pub enabled: bool,
    pub settings: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StreamRecordStatus {
    pub active: bool,
    pub paused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VideoSettings {
    pub base_width: u32,
    pub base_height: u32,
    pub output_width: u32,
    pub output_height: u32,
    pub fps_numerator: u32,
    pub fps_denominator: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StreamServiceSettings {
    pub service_type: String,
    pub server: String,
    pub key_set: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RecordSettings {
    pub record_directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ObsStats {
    pub active_fps: f64,
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub render_skipped_frames: u64,
    pub output_skipped_frames: u64,
}

impl ObsState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

pub async fn populate_initial_state(
    conn: &ObsConnection,
    state: &SharedObsState,
) -> Result<(), String> {
    let scene_data = conn.send_request("GetSceneList", None).await?;
    let current_scene = scene_data["currentProgramSceneName"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let scenes: Vec<SceneInfo> = scene_data["scenes"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .enumerate()
                .map(|(i, s)| SceneInfo {
                    name: s["sceneName"].as_str().unwrap_or("").to_string(),
                    index: i as u32,
                })
                .collect()
        })
        .unwrap_or_default();

    let special_data = conn.send_request("GetSpecialInputs", None).await.ok();
    let special_inputs = special_data
        .as_ref()
        .map(|v| SpecialInputs {
            desktop1: v["desktop1"].as_str().unwrap_or("").to_string(),
            desktop2: v["desktop2"].as_str().unwrap_or("").to_string(),
            mic1: v["mic1"].as_str().unwrap_or("").to_string(),
            mic2: v["mic2"].as_str().unwrap_or("").to_string(),
            mic3: v["mic3"].as_str().unwrap_or("").to_string(),
        })
        .unwrap_or_default();

    let input_data = conn.send_request("GetInputList", None).await?;
    let mut inputs = HashMap::new();

    if let Some(input_list) = input_data["inputs"].as_array() {
        for input in input_list {
            let name = input["inputName"].as_str().unwrap_or("").to_string();
            let kind = input["inputKind"].as_str().unwrap_or("").to_string();

            let (volume_db, volume_mul) = conn
                .send_request("GetInputVolume", Some(json!({"inputName": &name})))
                .await
                .ok()
                .map(|v| {
                    (
                        v["inputVolumeDb"].as_f64().unwrap_or(0.0),
                        v["inputVolumeMul"].as_f64().unwrap_or(1.0),
                    )
                })
                .unwrap_or((0.0, 1.0));

            let muted = conn
                .send_request("GetInputMute", Some(json!({"inputName": &name})))
                .await
                .ok()
                .and_then(|v| v["inputMuted"].as_bool())
                .unwrap_or(false);

            let monitor_type = conn
                .send_request(
                    "GetInputAudioMonitorType",
                    Some(json!({"inputName": &name})),
                )
                .await
                .ok()
                .and_then(|v| v["monitorType"].as_str().map(String::from))
                .unwrap_or_default();

            let filters = conn
                .send_request("GetSourceFilterList", Some(json!({"sourceName": &name})))
                .await
                .ok()
                .and_then(|v| {
                    v["filters"].as_array().map(|arr| {
                        arr.iter()
                            .map(|f| FilterInfo {
                                name: f["filterName"].as_str().unwrap_or("").to_string(),
                                kind: f["filterKind"].as_str().unwrap_or("").to_string(),
                                enabled: f["filterEnabled"].as_bool().unwrap_or(true),
                                settings: f["filterSettings"].clone(),
                            })
                            .collect()
                    })
                })
                .unwrap_or_default();

            let device_id = if kind.contains("wasapi_input_capture")
                || kind.contains("wasapi_output_capture")
            {
                conn.send_request(
                    "GetInputSettings",
                    Some(json!({"inputName": &name})),
                )
                .await
                .ok()
                .and_then(|v| {
                    v["inputSettings"]["device_id"]
                        .as_str()
                        .map(String::from)
                })
                .unwrap_or_else(|| "default".to_string())
            } else {
                String::new()
            };

            let audio_balance = conn
                .send_request("GetInputAudioBalance", Some(json!({"inputName": &name})))
                .await
                .ok()
                .and_then(|v| v["inputAudioBalance"].as_f64())
                .unwrap_or(0.5);

            let audio_sync_offset = conn
                .send_request("GetInputAudioSyncOffset", Some(json!({"inputName": &name})))
                .await
                .ok()
                .and_then(|v| v["inputAudioSyncOffset"].as_i64())
                .unwrap_or(0);

            let audio_tracks = conn
                .send_request("GetInputAudioTracks", Some(json!({"inputName": &name})))
                .await
                .ok()
                .and_then(|v| v.get("inputAudioTracks").cloned())
                .unwrap_or(json!({"1":true,"2":true,"3":false,"4":false,"5":false,"6":false}));

            inputs.insert(
                name.clone(),
                InputInfo {
                    name,
                    kind,
                    volume_db,
                    volume_mul,
                    muted,
                    monitor_type,
                    filters,
                    device_id,
                    audio_balance,
                    audio_sync_offset,
                    audio_tracks,
                },
            );
        }
    }

    // Also populate special inputs (global audio sources) that might not appear in GetInputList
    let special_names = [
        &special_inputs.desktop1,
        &special_inputs.desktop2,
        &special_inputs.mic1,
        &special_inputs.mic2,
        &special_inputs.mic3,
    ];
    for sname in special_names {
        if sname.is_empty() || inputs.contains_key(sname) {
            continue;
        }
        let (volume_db, volume_mul) = conn
            .send_request("GetInputVolume", Some(json!({"inputName": sname})))
            .await
            .ok()
            .map(|v| {
                (
                    v["inputVolumeDb"].as_f64().unwrap_or(0.0),
                    v["inputVolumeMul"].as_f64().unwrap_or(1.0),
                )
            })
            .unwrap_or((0.0, 1.0));

        let muted = conn
            .send_request("GetInputMute", Some(json!({"inputName": sname})))
            .await
            .ok()
            .and_then(|v| v["inputMuted"].as_bool())
            .unwrap_or(false);

        let monitor_type = conn
            .send_request(
                "GetInputAudioMonitorType",
                Some(json!({"inputName": sname})),
            )
            .await
            .ok()
            .and_then(|v| v["monitorType"].as_str().map(String::from))
            .unwrap_or_default();

        let filters = conn
            .send_request("GetSourceFilterList", Some(json!({"sourceName": sname})))
            .await
            .ok()
            .and_then(|v| {
                v["filters"].as_array().map(|arr| {
                    arr.iter()
                        .map(|f| FilterInfo {
                            name: f["filterName"].as_str().unwrap_or("").to_string(),
                            kind: f["filterKind"].as_str().unwrap_or("").to_string(),
                            enabled: f["filterEnabled"].as_bool().unwrap_or(true),
                            settings: f["filterSettings"].clone(),
                        })
                        .collect()
                })
            })
            .unwrap_or_default();

        let kind_result = conn
            .send_request("GetInputSettings", Some(json!({"inputName": sname})))
            .await
            .ok();
        let kind = kind_result
            .as_ref()
            .and_then(|v| v["inputKind"].as_str().map(String::from))
            .unwrap_or_default();
        let device_id = kind_result
            .as_ref()
            .and_then(|v| v["inputSettings"]["device_id"].as_str().map(String::from))
            .unwrap_or_else(|| "default".to_string());

        let audio_balance = conn
            .send_request("GetInputAudioBalance", Some(json!({"inputName": sname})))
            .await
            .ok()
            .and_then(|v| v["inputAudioBalance"].as_f64())
            .unwrap_or(0.5);

        let audio_sync_offset = conn
            .send_request("GetInputAudioSyncOffset", Some(json!({"inputName": sname})))
            .await
            .ok()
            .and_then(|v| v["inputAudioSyncOffset"].as_i64())
            .unwrap_or(0);

        let audio_tracks = conn
            .send_request("GetInputAudioTracks", Some(json!({"inputName": sname})))
            .await
            .ok()
            .and_then(|v| v.get("inputAudioTracks").cloned())
            .unwrap_or(json!({"1":true,"2":true,"3":false,"4":false,"5":false,"6":false}));

        inputs.insert(
            sname.clone(),
            InputInfo {
                name: sname.clone(),
                kind,
                volume_db,
                volume_mul,
                muted,
                monitor_type,
                filters,
                device_id,
                audio_balance,
                audio_sync_offset,
                audio_tracks,
            },
        );
    }

    let stream_status = conn
        .send_request("GetStreamStatus", None)
        .await
        .ok()
        .map(|v| StreamRecordStatus {
            active: v["outputActive"].as_bool().unwrap_or(false),
            paused: false,
        })
        .unwrap_or_default();

    let record_status = conn
        .send_request("GetRecordStatus", None)
        .await
        .ok()
        .map(|v| StreamRecordStatus {
            active: v["outputActive"].as_bool().unwrap_or(false),
            paused: v["outputPaused"].as_bool().unwrap_or(false),
        })
        .unwrap_or_default();

    let stats_data = conn.send_request("GetStats", None).await.ok();
    let stats = stats_data
        .map(|v| ObsStats {
            active_fps: v["activeFps"].as_f64().unwrap_or(0.0),
            cpu_usage: v["cpuUsage"].as_f64().unwrap_or(0.0),
            memory_usage: v["memoryUsage"].as_f64().unwrap_or(0.0),
            render_skipped_frames: v["renderSkippedFrames"].as_u64().unwrap_or(0),
            output_skipped_frames: v["outputSkippedFrames"].as_u64().unwrap_or(0),
        })
        .unwrap_or_default();

    let video_settings = conn
        .send_request("GetVideoSettings", None)
        .await
        .ok()
        .map(|v| VideoSettings {
            base_width: v["baseWidth"].as_u64().unwrap_or(0) as u32,
            base_height: v["baseHeight"].as_u64().unwrap_or(0) as u32,
            output_width: v["outputWidth"].as_u64().unwrap_or(0) as u32,
            output_height: v["outputHeight"].as_u64().unwrap_or(0) as u32,
            fps_numerator: v["fpsNumerator"].as_u64().unwrap_or(0) as u32,
            fps_denominator: v["fpsDenominator"].as_u64().unwrap_or(1) as u32,
        })
        .unwrap_or_default();

    let stream_service = conn
        .send_request("GetStreamServiceSettings", None)
        .await
        .ok()
        .map(|v| {
            let settings = &v["streamServiceSettings"];
            StreamServiceSettings {
                service_type: v["streamServiceType"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
                server: settings["server"].as_str().unwrap_or("").to_string(),
                key_set: settings["key"]
                    .as_str()
                    .map(|k| !k.is_empty())
                    .unwrap_or(false),
            }
        })
        .unwrap_or_default();

    let record_settings = conn
        .send_request("GetRecordDirectory", None)
        .await
        .ok()
        .map(|v| RecordSettings {
            record_directory: v["recordDirectory"]
                .as_str()
                .unwrap_or("")
                .to_string(),
        })
        .unwrap_or_default();

    let mut scene_items = HashMap::new();
    for scene in &scenes {
        if let Ok(items_data) = conn
            .send_request(
                "GetSceneItemList",
                Some(json!({"sceneName": &scene.name})),
            )
            .await
        {
            let items: Vec<SceneItemInfo> = items_data["sceneItems"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .map(|item| SceneItemInfo {
                            source_name: item["sourceName"]
                                .as_str()
                                .unwrap_or("")
                                .to_string(),
                            source_kind: item["inputKind"]
                                .as_str()
                                .or_else(|| item["sourceType"].as_str())
                                .unwrap_or("")
                                .to_string(),
                            scene_item_id: item["sceneItemId"].as_u64().unwrap_or(0),
                            enabled: item["sceneItemEnabled"].as_bool().unwrap_or(true),
                        })
                        .collect()
                })
                .unwrap_or_default();
            scene_items.insert(scene.name.clone(), items);
        }
    }

    let mut s = state.write().await;
    s.scenes = scenes;
    s.current_scene = current_scene;
    s.inputs = inputs;
    s.stream_status = stream_status;
    s.record_status = record_status;
    s.stats = stats;
    s.video_settings = video_settings;
    s.stream_service = stream_service;
    s.record_settings = record_settings;
    s.special_inputs = special_inputs;
    s.scene_items = scene_items;

    Ok(())
}
