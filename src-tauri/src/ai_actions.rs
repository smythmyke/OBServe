use crate::audio;
use crate::gemini::AiAction;
use crate::obs_state::ObsState;
use crate::obs_websocket::ObsConnection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

pub type SharedUndoStack = Arc<RwLock<Vec<UndoEntry>>>;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoEntry {
    pub description: String,
    pub action_type: String,
    pub request_type: String,
    pub revert_params: Value,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    pub description: String,
    pub status: String,
    pub error: Option<String>,
    pub undoable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_action: Option<AiAction>,
}

pub async fn execute_actions(
    actions: &[AiAction],
    conn: &ObsConnection,
    obs_state: &ObsState,
    undo_stack: &SharedUndoStack,
) -> Vec<ActionResult> {
    let mut results = Vec::new();

    for action in actions {
        let result = match action.safety.as_str() {
            "dangerous" => ActionResult {
                description: action.description.clone(),
                status: "pending_confirmation".into(),
                error: None,
                undoable: false,
                pending_action: Some(action.clone()),
            },
            "caution" => {
                let undo = snapshot_for_undo(action, obs_state);
                let exec_result = dispatch_action(action, conn).await;
                if let (Ok(()), Some(undo_entry)) = (&exec_result, undo) {
                    undo_stack.write().await.push(undo_entry);
                }
                match exec_result {
                    Ok(()) => ActionResult {
                        description: action.description.clone(),
                        status: "executed".into(),
                        error: None,
                        undoable: true,
                        pending_action: None,
                    },
                    Err(e) => ActionResult {
                        description: action.description.clone(),
                        status: "failed".into(),
                        error: Some(e),
                        undoable: false,
                        pending_action: None,
                    },
                }
            }
            _ => {
                let exec_result = dispatch_action(action, conn).await;
                match exec_result {
                    Ok(()) => ActionResult {
                        description: action.description.clone(),
                        status: "executed".into(),
                        error: None,
                        undoable: false,
                        pending_action: None,
                    },
                    Err(e) => ActionResult {
                        description: action.description.clone(),
                        status: "failed".into(),
                        error: Some(e),
                        undoable: false,
                        pending_action: None,
                    },
                }
            }
        };
        results.push(result);
    }

    results
}

async fn dispatch_action(action: &AiAction, conn: &ObsConnection) -> Result<(), String> {
    match action.action_type.as_str() {
        "obs_request" => {
            if action.request_type == "SetSceneItemEnabled" {
                return dispatch_scene_item_enabled(action, conn).await;
            }
            let params = if action.params.as_object().map_or(true, |o| o.is_empty()) {
                None
            } else {
                Some(action.params.clone())
            };
            conn.send_request(&action.request_type, params).await?;
            Ok(())
        }
        "windows_audio" => {
            let params = action.params.clone();
            let request_type = action.request_type.clone();
            tokio::task::spawn_blocking(move || match request_type.as_str() {
                "set_volume" => {
                    let device_id = params["deviceId"]
                        .as_str()
                        .ok_or("Missing deviceId")?;
                    let volume = params["volume"]
                        .as_f64()
                        .ok_or("Missing volume")? as f32;
                    audio::set_device_volume(device_id, volume)
                }
                "set_mute" => {
                    let device_id = params["deviceId"]
                        .as_str()
                        .ok_or("Missing deviceId")?;
                    let muted = params["muted"]
                        .as_bool()
                        .ok_or("Missing muted")?;
                    audio::set_device_mute(device_id, muted)
                }
                other => Err(format!("Unknown windows_audio command: {}", other)),
            })
            .await
            .map_err(|e| format!("Task failed: {}", e))?
        }
        other => Err(format!("Unknown action_type: {}", other)),
    }
}

fn snapshot_for_undo(action: &AiAction, obs_state: &ObsState) -> Option<UndoEntry> {
    match action.action_type.as_str() {
        "obs_request" => match action.request_type.as_str() {
            "SetInputVolume" => {
                let input_name = action.params["inputName"].as_str()?;
                let input = obs_state.inputs.get(input_name)?;
                Some(UndoEntry {
                    description: format!("Revert volume of \"{}\"", input_name),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    revert_params: json!({
                        "inputName": input_name,
                        "inputVolumeDb": input.volume_db
                    }),
                })
            }
            "SetInputMute" | "ToggleInputMute" => {
                let input_name = action.params["inputName"].as_str()?;
                let input = obs_state.inputs.get(input_name)?;
                Some(UndoEntry {
                    description: format!("Revert mute state of \"{}\"", input_name),
                    action_type: "obs_request".into(),
                    request_type: "SetInputMute".into(),
                    revert_params: json!({
                        "inputName": input_name,
                        "inputMuted": input.muted
                    }),
                })
            }
            "SetCurrentProgramScene" => Some(UndoEntry {
                description: format!("Revert to scene \"{}\"", obs_state.current_scene),
                action_type: "obs_request".into(),
                request_type: "SetCurrentProgramScene".into(),
                revert_params: json!({
                    "sceneName": obs_state.current_scene
                }),
            }),
            "CreateSourceFilter" => {
                let source_name = action.params["sourceName"].as_str()?;
                let filter_name = action.params["filterName"].as_str()?;
                Some(UndoEntry {
                    description: format!("Remove filter \"{}\" from \"{}\"", filter_name, source_name),
                    action_type: "obs_request".into(),
                    request_type: "RemoveSourceFilter".into(),
                    revert_params: json!({
                        "sourceName": source_name,
                        "filterName": filter_name
                    }),
                })
            }
            "SetSceneItemEnabled" => {
                let scene_name = action.params["sceneName"].as_str()?;
                let source_name = action.params["sourceName"].as_str()?;
                let enabled = action.params["sceneItemEnabled"].as_bool()?;
                Some(UndoEntry {
                    description: format!(
                        "Revert \"{}\" to {}",
                        source_name,
                        if enabled { "hidden" } else { "visible" }
                    ),
                    action_type: "obs_request".into(),
                    request_type: "SetSceneItemEnabled".into(),
                    revert_params: json!({
                        "sceneName": scene_name,
                        "sourceName": source_name,
                        "sceneItemEnabled": !enabled
                    }),
                })
            }
            "SetSourceFilterSettings" => {
                let source_name = action.params["sourceName"].as_str()?;
                let filter_name = action.params["filterName"].as_str()?;
                let input = obs_state.inputs.get(source_name)?;
                let filter = input.filters.iter().find(|f| f.name == filter_name)?;
                Some(UndoEntry {
                    description: format!(
                        "Revert settings of filter \"{}\" on \"{}\"",
                        filter_name, source_name
                    ),
                    action_type: "obs_request".into(),
                    request_type: "SetSourceFilterSettings".into(),
                    revert_params: json!({
                        "sourceName": source_name,
                        "filterName": filter_name,
                        "filterSettings": filter.settings
                    }),
                })
            }
            "RemoveSourceFilter" => {
                let source_name = action.params["sourceName"].as_str()?;
                let filter_name = action.params["filterName"].as_str()?;
                let input = obs_state.inputs.get(source_name)?;
                let filter = input.filters.iter().find(|f| f.name == filter_name)?;
                Some(UndoEntry {
                    description: format!("Re-add filter \"{}\" to \"{}\"", filter_name, source_name),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    revert_params: json!({
                        "sourceName": source_name,
                        "filterName": filter_name,
                        "filterKind": filter.kind,
                        "filterSettings": filter.settings
                    }),
                })
            }
            _ => None,
        },
        _ => None,
    }
}

async fn dispatch_scene_item_enabled(action: &AiAction, conn: &ObsConnection) -> Result<(), String> {
    let scene_name = action.params["sceneName"]
        .as_str()
        .ok_or("Missing sceneName")?;
    let source_name = action.params["sourceName"]
        .as_str()
        .ok_or("Missing sourceName")?;
    let enabled = action.params["sceneItemEnabled"]
        .as_bool()
        .ok_or("Missing sceneItemEnabled")?;

    let items_data = conn
        .send_request(
            "GetSceneItemList",
            Some(json!({"sceneName": scene_name})),
        )
        .await?;

    let scene_item_id = items_data["sceneItems"]
        .as_array()
        .and_then(|arr| {
            arr.iter().find_map(|item| {
                if item["sourceName"].as_str() == Some(source_name) {
                    item["sceneItemId"].as_u64()
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| format!("Source \"{}\" not found in scene \"{}\"", source_name, scene_name))?;

    conn.send_request(
        "SetSceneItemEnabled",
        Some(json!({
            "sceneName": scene_name,
            "sceneItemId": scene_item_id,
            "sceneItemEnabled": enabled
        })),
    )
    .await?;
    Ok(())
}

pub async fn execute_single_action(
    action: &AiAction,
    conn: &ObsConnection,
) -> Result<(), String> {
    dispatch_action(action, conn).await
}

pub async fn undo_last(
    conn: &ObsConnection,
    undo_stack: &SharedUndoStack,
) -> Result<String, String> {
    let entry = {
        let mut stack = undo_stack.write().await;
        stack.pop().ok_or_else(|| "Nothing to undo".to_string())?
    };

    let undo_action = AiAction {
        safety: "safe".into(),
        description: entry.description.clone(),
        action_type: entry.action_type,
        request_type: entry.request_type,
        params: entry.revert_params,
    };

    dispatch_action(&undo_action, conn).await?;
    Ok(entry.description)
}
