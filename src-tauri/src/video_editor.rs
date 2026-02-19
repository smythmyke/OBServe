use crate::obs_launcher;
use crate::obs_state::SharedObsState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

pub type SharedVideoEditorState = Arc<Mutex<VideoEditorState>>;

pub struct VideoEditorState {
    pub ffmpeg_path: Option<PathBuf>,
    pub ffprobe_path: Option<PathBuf>,
    pub temp_dir: PathBuf,
    pub thumbnail_cache: HashMap<String, String>,
    pub export_progress: ExportProgress,
    pub export_cancel: Arc<AtomicBool>,
}

impl VideoEditorState {
    pub fn new() -> Self {
        let temp_dir = std::env::temp_dir().join("observe-video-editor");
        let _ = std::fs::create_dir_all(&temp_dir);
        let _ = std::fs::create_dir_all(temp_dir.join("thumbnails"));
        let _ = std::fs::create_dir_all(temp_dir.join("remuxed"));
        Self {
            ffmpeg_path: None,
            ffprobe_path: None,
            temp_dir,
            thumbnail_cache: HashMap::new(),
            export_progress: ExportProgress::default(),
            export_cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

// ---- Serializable Types ----

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FfmpegStatus {
    pub found: bool,
    pub path: Option<String>,
    pub ffprobe_path: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VideoFileInfo {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub modified: u64,
    pub extension: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub video_codec: String,
    pub audio_codec: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub deleted: bool,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Overlay {
    pub id: String,
    pub overlay_type: String,
    pub content: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub start_time: f64,
    pub end_time: f64,
    pub style: OverlayStyle,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OverlayStyle {
    pub font_size: u32,
    pub font_color: String,
    pub background_color: String,
    pub opacity: f64,
    pub bold: bool,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub source_path: String,
    pub segments: Vec<Segment>,
    pub overlays: Vec<Overlay>,
    pub output_path: String,
    pub format: String,
    pub video_codec: String,
    pub quality: String,
    pub resolution: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    pub percent: f64,
    pub eta_seconds: f64,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EditProjectSave {
    pub source_path: String,
    pub segments: Vec<Segment>,
    pub overlays: Vec<Overlay>,
    pub duration: f64,
}

// ---- FFmpeg Detection ----

fn find_ffmpeg_from_obs() -> Option<PathBuf> {
    let obs_exe = obs_launcher::find_obs_path()?;
    let bin_dir = obs_exe.parent()?;
    let ffmpeg = bin_dir.join("ffmpeg.exe");
    if ffmpeg.exists() {
        Some(ffmpeg)
    } else {
        None
    }
}

fn find_ffmpeg_in_path() -> Option<PathBuf> {
    // First try the process PATH (works if app was launched after install)
    let output = std::process::Command::new("where")
        .arg("ffmpeg.exe")
        .output()
        .ok()?;
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let first_line = stdout.lines().next()?.trim();
        let path = PathBuf::from(first_line);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

fn find_ffmpeg_in_live_path() -> Option<PathBuf> {
    // Read live system+user PATH from registry/environment (catches installs after app launch)
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command",
            r#"$p = [Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [Environment]::GetEnvironmentVariable('Path','User'); foreach ($d in $p -split ';') { $f = Join-Path $d 'ffmpeg.exe'; if (Test-Path $f) { $f; break } }"#])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.trim();
    if !line.is_empty() {
        let path = PathBuf::from(line);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

fn find_ffmpeg_known_paths() -> Option<PathBuf> {
    let mut known = vec![
        r"C:\ffmpeg\bin\ffmpeg.exe".to_string(),
        r"C:\Program Files\ffmpeg\bin\ffmpeg.exe".to_string(),
        r"C:\tools\ffmpeg\bin\ffmpeg.exe".to_string(),
        r"C:\ProgramData\chocolatey\bin\ffmpeg.exe".to_string(),
    ];
    if let Some(home) = dirs::home_dir() {
        known.push(format!(r"{}\scoop\shims\ffmpeg.exe", home.display()));
        // Scan winget packages dir for any FFmpeg version
        let winget_dir = home.join(r"AppData\Local\Microsoft\WinGet\Packages");
        if winget_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&winget_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    if name.to_string_lossy().starts_with("Gyan.FFmpeg") {
                        if let Ok(sub) = std::fs::read_dir(entry.path()) {
                            for sub_entry in sub.flatten() {
                                let ffmpeg = sub_entry.path().join("bin").join("ffmpeg.exe");
                                if ffmpeg.exists() {
                                    return Some(ffmpeg);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    for p in &known {
        let path = PathBuf::from(p);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

fn detect_ffmpeg_inner() -> (Option<PathBuf>, Option<PathBuf>) {
    let ffmpeg = find_ffmpeg_from_obs()
        .or_else(find_ffmpeg_in_path)
        .or_else(find_ffmpeg_known_paths)
        .or_else(find_ffmpeg_in_live_path);

    let ffprobe = ffmpeg.as_ref().and_then(|ff| {
        let dir = ff.parent()?;
        let probe = dir.join("ffprobe.exe");
        if probe.exists() {
            Some(probe)
        } else {
            None
        }
    });

    (ffmpeg, ffprobe)
}

// ---- Phase 1: Player + Auto-Load Commands ----

#[tauri::command]
pub async fn detect_ffmpeg(
    state: tauri::State<'_, SharedVideoEditorState>,
) -> Result<FfmpegStatus, String> {
    let (ffmpeg, ffprobe) = tokio::task::spawn_blocking(detect_ffmpeg_inner)
        .await
        .map_err(|e| format!("Task failed: {}", e))?;

    let mut s = state.lock().await;
    s.ffmpeg_path = ffmpeg.clone();
    s.ffprobe_path = ffprobe.clone();

    Ok(FfmpegStatus {
        found: ffmpeg.is_some(),
        path: ffmpeg.map(|p| p.to_string_lossy().to_string()),
        ffprobe_path: ffprobe.map(|p| p.to_string_lossy().to_string()),
    })
}

#[tauri::command]
pub async fn list_recordings(
    obs_state: tauri::State<'_, SharedObsState>,
    dir: Option<String>,
) -> Result<Vec<VideoFileInfo>, String> {
    let recording_dir = match dir {
        Some(d) if !d.is_empty() => d,
        _ => {
            let s = obs_state.read().await;
            let d = s.record_settings.record_directory.clone();
            if d.is_empty() {
                return Err("No recording directory configured in OBS".to_string());
            }
            d
        }
    };

    tokio::task::spawn_blocking(move || list_video_files(&recording_dir))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

fn list_video_files(dir: &str) -> Result<Vec<VideoFileInfo>, String> {
    let path = Path::new(dir);
    if !path.exists() {
        return Err(format!("Directory not found: {}", dir));
    }

    let extensions = ["mkv", "mp4", "flv", "mov", "ts", "webm"];
    let mut files: Vec<VideoFileInfo> = std::fs::read_dir(path)
        .map_err(|e| format!("Cannot read directory: {}", e))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            let ext = path.extension()?.to_str()?.to_lowercase();
            if !extensions.contains(&ext.as_str()) {
                return None;
            }
            let meta = entry.metadata().ok()?;
            let modified = meta
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_secs();
            Some(VideoFileInfo {
                path: path.to_string_lossy().to_string(),
                name: path.file_name()?.to_string_lossy().to_string(),
                size_bytes: meta.len(),
                modified,
                extension: ext,
            })
        })
        .collect();

    files.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(files)
}

#[tauri::command]
pub async fn remux_to_mp4(
    state: tauri::State<'_, SharedVideoEditorState>,
    source_path: String,
) -> Result<String, String> {
    let src = PathBuf::from(&source_path);
    if !src.exists() {
        return Err(format!("File not found: {}", source_path));
    }

    let ext = src
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if ext == "mp4" {
        return Ok(source_path);
    }

    let s = state.lock().await;
    let ffmpeg = s
        .ffmpeg_path
        .clone()
        .ok_or("FFmpeg not found. Run detect_ffmpeg first.")?;
    let remux_dir = s.temp_dir.join("remuxed");
    drop(s);

    let stem = src
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "video".to_string());
    let output = remux_dir.join(format!("{}.mp4", stem));

    if output.exists() {
        let src_modified = std::fs::metadata(&src)
            .and_then(|m| m.modified())
            .ok();
        let out_modified = std::fs::metadata(&output)
            .and_then(|m| m.modified())
            .ok();
        if let (Some(s), Some(o)) = (src_modified, out_modified) {
            if o > s {
                return Ok(output.to_string_lossy().to_string());
            }
        }
    }

    let output_str = output.to_string_lossy().to_string();
    let result = tokio::process::Command::new(&ffmpeg)
        .args([
            "-y",
            "-i",
            &source_path,
            "-c",
            "copy",
            "-movflags",
            "+faststart",
            &output_str,
        ])
        .output()
        .await
        .map_err(|e| format!("FFmpeg failed to start: {}", e))?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(format!("Remux failed: {}", stderr));
    }

    Ok(output_str)
}

#[tauri::command]
pub async fn get_video_info(
    state: tauri::State<'_, SharedVideoEditorState>,
    path: String,
) -> Result<VideoInfo, String> {
    let s = state.lock().await;
    let ffprobe = s
        .ffprobe_path
        .clone()
        .ok_or("ffprobe not found. Run detect_ffmpeg first.")?;
    drop(s);

    let result = tokio::process::Command::new(&ffprobe)
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            &path,
        ])
        .output()
        .await
        .map_err(|e| format!("ffprobe failed: {}", e))?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(format!("ffprobe error: {}", stderr));
    }

    let json: Value = serde_json::from_slice(&result.stdout)
        .map_err(|e| format!("Invalid ffprobe output: {}", e))?;

    let streams = json["streams"].as_array();
    let format = &json["format"];

    let mut video_codec = String::new();
    let mut audio_codec = String::new();
    let mut width = 0u32;
    let mut height = 0u32;
    let mut fps = 0.0f64;

    if let Some(streams) = streams {
        for stream in streams {
            match stream["codec_type"].as_str() {
                Some("video") => {
                    video_codec = stream["codec_name"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    width = stream["width"].as_u64().unwrap_or(0) as u32;
                    height = stream["height"].as_u64().unwrap_or(0) as u32;
                    if let Some(r_frame_rate) = stream["r_frame_rate"].as_str() {
                        let parts: Vec<&str> = r_frame_rate.split('/').collect();
                        if parts.len() == 2 {
                            let num: f64 = parts[0].parse().unwrap_or(0.0);
                            let den: f64 = parts[1].parse().unwrap_or(1.0);
                            if den > 0.0 {
                                fps = num / den;
                            }
                        }
                    }
                }
                Some("audio") => {
                    audio_codec = stream["codec_name"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                }
                _ => {}
            }
        }
    }

    let duration = format["duration"]
        .as_str()
        .and_then(|d| d.parse::<f64>().ok())
        .unwrap_or(0.0);

    Ok(VideoInfo {
        duration,
        width,
        height,
        fps,
        video_codec,
        audio_codec,
    })
}

// ---- Phase 2: Browser + File Management Commands ----

#[tauri::command]
pub async fn get_video_thumbnail(
    state: tauri::State<'_, SharedVideoEditorState>,
    path: String,
    timestamp: f64,
) -> Result<String, String> {
    let cache_key = format!(
        "{}_{:.0}",
        path.replace(['\\', '/', ':', '.'], "_"),
        timestamp * 1000.0
    );

    {
        let s = state.lock().await;
        if let Some(cached) = s.thumbnail_cache.get(&cache_key) {
            return Ok(cached.clone());
        }
    }

    let s = state.lock().await;
    let ffmpeg = s
        .ffmpeg_path
        .clone()
        .ok_or("FFmpeg not found")?;
    let thumb_dir = s.temp_dir.join("thumbnails");
    drop(s);

    let thumb_path = thumb_dir.join(format!("{}.png", cache_key));
    let thumb_str = thumb_path.to_string_lossy().to_string();
    let ts_str = format!("{:.3}", timestamp);

    let result = tokio::process::Command::new(&ffmpeg)
        .args([
            "-y",
            "-ss",
            &ts_str,
            "-i",
            &path,
            "-vframes",
            "1",
            "-vf",
            "scale=320:-1",
            &thumb_str,
        ])
        .output()
        .await
        .map_err(|e| format!("FFmpeg thumbnail failed: {}", e))?;

    if !result.status.success() {
        return Err("Failed to extract thumbnail".to_string());
    }

    let png_data = std::fs::read(&thumb_path)
        .map_err(|e| format!("Failed to read thumbnail: {}", e))?;
    let base64 = format!(
        "data:image/png;base64,{}",
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &png_data)
    );

    let mut s = state.lock().await;
    s.thumbnail_cache.insert(cache_key, base64.clone());

    Ok(base64)
}

#[tauri::command]
pub async fn open_file_location(path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open explorer: {}", e))?;
        Ok(())
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
pub async fn delete_recording(path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let escaped = path.replace('\'', "''");
        let ps_script = format!(
            "Add-Type -AssemblyName Microsoft.VisualBasic; \
             [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile(\
             '{}', 'UIOption.OnlyErrorDialogs', 'RecycleOption.SendToRecycleBin')",
            escaped
        );
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps_script])
            .output()
            .map_err(|e| format!("Failed to run PowerShell: {}", e))?;

        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "Delete failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

// ---- Phase 3: Preview Edit ----

#[tauri::command]
pub async fn preview_edit(
    state: tauri::State<'_, SharedVideoEditorState>,
    source_path: String,
    segments: Vec<Segment>,
) -> Result<String, String> {
    let s = state.lock().await;
    let ffmpeg = s
        .ffmpeg_path
        .clone()
        .ok_or("FFmpeg not found")?;
    let temp_dir = s.temp_dir.clone();
    drop(s);

    let active_segments: Vec<&Segment> = segments.iter().filter(|s| !s.deleted).collect();
    if active_segments.is_empty() {
        return Err("No segments to preview".to_string());
    }

    let output = temp_dir.join("preview.mp4");
    let output_str = output.to_string_lossy().to_string();

    if active_segments.len() == 1 {
        let seg = active_segments[0];
        let result = tokio::process::Command::new(&ffmpeg)
            .args([
                "-y",
                "-ss",
                &format!("{:.3}", seg.start),
                "-to",
                &format!("{:.3}", seg.end),
                "-i",
                &source_path,
                "-c",
                "copy",
                "-movflags",
                "+faststart",
                &output_str,
            ])
            .output()
            .await
            .map_err(|e| format!("FFmpeg failed: {}", e))?;

        if !result.status.success() {
            return Err(format!(
                "Preview failed: {}",
                String::from_utf8_lossy(&result.stderr)
            ));
        }
        return Ok(output_str);
    }

    let concat_file = temp_dir.join("concat.txt");
    let mut temp_files = Vec::new();

    for (i, seg) in active_segments.iter().enumerate() {
        let seg_file = temp_dir.join(format!("seg_{}.mp4", i));
        let seg_str = seg_file.to_string_lossy().to_string();

        let result = tokio::process::Command::new(&ffmpeg)
            .args([
                "-y",
                "-ss",
                &format!("{:.3}", seg.start),
                "-to",
                &format!("{:.3}", seg.end),
                "-i",
                &source_path,
                "-c",
                "copy",
                &seg_str,
            ])
            .output()
            .await
            .map_err(|e| format!("FFmpeg segment failed: {}", e))?;

        if !result.status.success() {
            for f in &temp_files {
                let _ = std::fs::remove_file(f);
            }
            return Err(format!(
                "Segment {} failed: {}",
                i,
                String::from_utf8_lossy(&result.stderr)
            ));
        }
        temp_files.push(seg_file.clone());
    }

    let concat_content: String = temp_files
        .iter()
        .map(|f| format!("file '{}'", f.to_string_lossy().replace('\'', "'\\''")))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&concat_file, &concat_content)
        .map_err(|e| format!("Failed to write concat file: {}", e))?;

    let concat_str = concat_file.to_string_lossy().to_string();
    let result = tokio::process::Command::new(&ffmpeg)
        .args([
            "-y",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            &concat_str,
            "-c",
            "copy",
            "-movflags",
            "+faststart",
            &output_str,
        ])
        .output()
        .await
        .map_err(|e| format!("FFmpeg concat failed: {}", e))?;

    for f in &temp_files {
        let _ = std::fs::remove_file(f);
    }
    let _ = std::fs::remove_file(&concat_file);

    if !result.status.success() {
        return Err(format!(
            "Preview concat failed: {}",
            String::from_utf8_lossy(&result.stderr)
        ));
    }

    Ok(output_str)
}

// ---- Phase 4: Image File Picker ----

#[tauri::command]
pub async fn pick_image_file() -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(|| {
        let ps_script = r#"
            Add-Type -AssemblyName System.Windows.Forms
            $dialog = New-Object System.Windows.Forms.OpenFileDialog
            $dialog.Filter = 'Images|*.png;*.jpg;*.jpeg;*.gif;*.bmp;*.webp|All files|*.*'
            $dialog.Title = 'Select Image for Overlay'
            if ($dialog.ShowDialog() -eq 'OK') { $dialog.FileName } else { '' }
        "#;
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", ps_script])
            .output()
            .map_err(|e| format!("Failed to run file dialog: {}", e))?;

        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(path))
        }
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

// ---- Phase 5: Export Engine ----

#[tauri::command]
pub async fn export_video(
    state: tauri::State<'_, SharedVideoEditorState>,
    app_handle: tauri::AppHandle,
    request: ExportRequest,
) -> Result<(), String> {
    let s = state.lock().await;
    let ffmpeg = s
        .ffmpeg_path
        .clone()
        .ok_or("FFmpeg not found")?;
    let temp_dir = s.temp_dir.clone();
    let cancel_flag = s.export_cancel.clone();
    drop(s);

    cancel_flag.store(false, Ordering::SeqCst);

    let state_clone = state.inner().clone();
    let active_segments: Vec<Segment> = request
        .segments
        .iter()
        .filter(|s| !s.deleted)
        .cloned()
        .collect();

    if active_segments.is_empty() {
        return Err("No segments to export".to_string());
    }

    {
        let mut s = state_clone.lock().await;
        s.export_progress = ExportProgress {
            percent: 0.0,
            eta_seconds: 0.0,
            status: "starting".to_string(),
            error: None,
        };
    }

    tauri::async_runtime::spawn(async move {
        let result = run_export(
            &ffmpeg,
            &temp_dir,
            &request,
            &active_segments,
            &state_clone,
            &cancel_flag,
            &app_handle,
        )
        .await;

        let mut s = state_clone.lock().await;
        match result {
            Ok(()) => {
                s.export_progress.percent = 100.0;
                s.export_progress.status = "done".to_string();
            }
            Err(e) => {
                if cancel_flag.load(Ordering::SeqCst) {
                    s.export_progress.status = "cancelled".to_string();
                } else {
                    s.export_progress.status = "error".to_string();
                    s.export_progress.error = Some(e);
                }
            }
        }
        let progress = s.export_progress.clone();
        drop(s);
        let _ = tauri::Emitter::emit(&app_handle, "video-editor://export-progress", &progress);
    });

    Ok(())
}

async fn run_export(
    ffmpeg: &Path,
    temp_dir: &Path,
    request: &ExportRequest,
    segments: &[Segment],
    state: &SharedVideoEditorState,
    cancel: &AtomicBool,
    app_handle: &tauri::AppHandle,
) -> Result<(), String> {
    let has_overlays = !request.overlays.is_empty();
    let crf = match request.quality.as_str() {
        "high" => "18",
        "low" => "28",
        _ => "23",
    };

    let total_duration: f64 = segments.iter().map(|s| s.end - s.start).sum();

    if !has_overlays && segments.len() == 1 {
        let seg = &segments[0];
        let mut cmd = tokio::process::Command::new(ffmpeg);
        cmd.args([
            "-y",
            "-progress",
            "pipe:1",
            "-ss",
            &format!("{:.3}", seg.start),
            "-to",
            &format!("{:.3}", seg.end),
            "-i",
            &request.source_path,
        ]);

        if request.video_codec == "libx264" || request.video_codec.contains("nvenc") || request.video_codec.contains("amf") || request.video_codec.contains("qsv") {
            cmd.args(["-c:v", &request.video_codec, "-crf", crf]);
        } else {
            cmd.args(["-c", "copy"]);
        }
        cmd.args(["-c:a", "aac", "-movflags", "+faststart", &request.output_path]);

        return run_ffmpeg_with_progress(cmd, total_duration, state, cancel, app_handle).await;
    }

    if !has_overlays {
        let seg_files = split_segments(ffmpeg, temp_dir, &request.source_path, segments, cancel).await?;
        let concat_file = temp_dir.join("export_concat.txt");
        let concat_content: String = seg_files
            .iter()
            .map(|f| format!("file '{}'", f.to_string_lossy().replace('\'', "'\\''")))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&concat_file, &concat_content)
            .map_err(|e| format!("Write concat file: {}", e))?;

        let mut cmd = tokio::process::Command::new(ffmpeg);
        cmd.args([
            "-y",
            "-progress",
            "pipe:1",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            &concat_file.to_string_lossy(),
            "-c",
            "copy",
            "-movflags",
            "+faststart",
            &request.output_path,
        ]);

        let result = run_ffmpeg_with_progress(cmd, total_duration, state, cancel, app_handle).await;

        for f in &seg_files {
            let _ = std::fs::remove_file(f);
        }
        let _ = std::fs::remove_file(&concat_file);
        return result;
    }

    let filter = build_filter_complex(segments, &request.overlays, &request.source_path);
    let mut cmd = tokio::process::Command::new(ffmpeg);
    cmd.args(["-y", "-progress", "pipe:1", "-i", &request.source_path]);

    for overlay in &request.overlays {
        if overlay.overlay_type == "image" && !overlay.content.is_empty() {
            cmd.args(["-i", &overlay.content]);
        }
    }

    cmd.args([
        "-filter_complex",
        &filter,
        "-map",
        "[vfinal]",
        "-map",
        "[afinal]",
        "-c:v",
        &request.video_codec,
        "-crf",
        crf,
        "-c:a",
        "aac",
        "-movflags",
        "+faststart",
        &request.output_path,
    ]);

    run_ffmpeg_with_progress(cmd, total_duration, state, cancel, app_handle).await
}

async fn split_segments(
    ffmpeg: &Path,
    temp_dir: &Path,
    source: &str,
    segments: &[Segment],
    cancel: &AtomicBool,
) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for (i, seg) in segments.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            for f in &files {
                let _ = std::fs::remove_file(f);
            }
            return Err("Export cancelled".to_string());
        }
        let seg_file = temp_dir.join(format!("export_seg_{}.mp4", i));
        let seg_str = seg_file.to_string_lossy().to_string();
        let result = tokio::process::Command::new(ffmpeg)
            .args([
                "-y",
                "-ss",
                &format!("{:.3}", seg.start),
                "-to",
                &format!("{:.3}", seg.end),
                "-i",
                source,
                "-c",
                "copy",
                &seg_str,
            ])
            .output()
            .await
            .map_err(|e| format!("Segment split failed: {}", e))?;

        if !result.status.success() {
            for f in &files {
                let _ = std::fs::remove_file(f);
            }
            return Err(format!(
                "Segment {} export failed: {}",
                i,
                String::from_utf8_lossy(&result.stderr)
            ));
        }
        files.push(seg_file);
    }
    Ok(files)
}

fn build_filter_complex(segments: &[Segment], overlays: &[Overlay], _source: &str) -> String {
    let mut filter = String::new();
    let n = segments.len();

    for (i, seg) in segments.iter().enumerate() {
        filter.push_str(&format!(
            "[0:v]trim=start={:.3}:end={:.3},setpts=PTS-STARTPTS[v{i}]; \
             [0:a]atrim=start={:.3}:end={:.3},asetpts=PTS-STARTPTS[a{i}]; ",
            seg.start, seg.end, seg.start, seg.end
        ));
    }

    let seg_labels: String = (0..n)
        .map(|i| format!("[v{i}][a{i}]"))
        .collect::<Vec<_>>()
        .join("");

    if n > 1 {
        filter.push_str(&format!(
            "{seg_labels}concat=n={n}:v=1:a=1[vconcat][aconcat]; "
        ));
    } else {
        filter.push_str("[v0]copy[vconcat]; [a0]copy[aconcat]; ");
    }

    let mut current_v = "vconcat".to_string();
    let mut input_idx = 1;
    let mut time_offset = 0.0_f64;

    let seg_offsets: Vec<f64> = {
        let mut offsets = vec![0.0];
        for seg in segments.iter().take(segments.len().saturating_sub(1)) {
            time_offset += seg.end - seg.start;
            offsets.push(time_offset);
        }
        offsets
    };
    let _ = seg_offsets;

    for (oi, overlay) in overlays.iter().enumerate() {
        let out_label = format!("ov{oi}");
        match overlay.overlay_type.as_str() {
            "text" => {
                let escaped = overlay
                    .content
                    .replace('\\', "\\\\")
                    .replace('\'', "'\\\\\\''")
                    .replace(':', "\\:");
                let color = &overlay.style.font_color;
                let size = overlay.style.font_size;
                let bold_str = if overlay.style.bold { ":bold=1" } else { "" };
                filter.push_str(&format!(
                    "[{current_v}]drawtext=text='{escaped}':fontsize={size}:\
                     fontcolor={color}:x={:.0}:y={:.0}{bold_str}:\
                     enable='between(t,{:.3},{:.3})'[{out_label}]; ",
                    overlay.x, overlay.y, overlay.start_time, overlay.end_time
                ));
            }
            "image" => {
                filter.push_str(&format!(
                    "[{input_idx}:v]scale={:.0}:{:.0}[img{oi}]; \
                     [{current_v}][img{oi}]overlay=x={:.0}:y={:.0}:\
                     enable='between(t,{:.3},{:.3})'[{out_label}]; ",
                    overlay.width,
                    overlay.height,
                    overlay.x,
                    overlay.y,
                    overlay.start_time,
                    overlay.end_time
                ));
                input_idx += 1;
            }
            _ => {
                filter.push_str(&format!("[{current_v}]copy[{out_label}]; "));
            }
        }
        current_v = out_label;
    }

    filter.push_str(&format!("[{current_v}]copy[vfinal]; [aconcat]copy[afinal]"));
    filter
}

async fn run_ffmpeg_with_progress(
    mut cmd: tokio::process::Command,
    total_duration: f64,
    state: &SharedVideoEditorState,
    cancel: &AtomicBool,
    app_handle: &tauri::AppHandle,
) -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt, BufReader};

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("FFmpeg spawn failed: {}", e))?;

    let stdout = child.stdout.take();
    let start_time = std::time::Instant::now();

    if let Some(stdout) = stdout {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            if cancel.load(Ordering::SeqCst) {
                let _ = child.kill().await;
                return Err("Export cancelled".to_string());
            }

            if line.starts_with("out_time_us=") {
                if let Ok(us) = line.trim_start_matches("out_time_us=").parse::<f64>() {
                    let current = us / 1_000_000.0;
                    let percent = if total_duration > 0.0 {
                        (current / total_duration * 100.0).min(99.9)
                    } else {
                        0.0
                    };
                    let elapsed = start_time.elapsed().as_secs_f64();
                    let eta = if percent > 0.0 {
                        (elapsed / percent * (100.0 - percent)).max(0.0)
                    } else {
                        0.0
                    };

                    let mut s = state.lock().await;
                    s.export_progress.percent = percent;
                    s.export_progress.eta_seconds = eta;
                    s.export_progress.status = "encoding".to_string();
                    let progress = s.export_progress.clone();
                    drop(s);
                    let _ = tauri::Emitter::emit(app_handle, "video-editor://export-progress", &progress);
                }
            }
        }
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("FFmpeg wait failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg export failed: {}", stderr));
    }

    Ok(())
}

#[tauri::command]
pub async fn get_export_progress(
    state: tauri::State<'_, SharedVideoEditorState>,
) -> Result<ExportProgress, String> {
    let s = state.lock().await;
    Ok(s.export_progress.clone())
}

#[tauri::command]
pub async fn cancel_export(
    state: tauri::State<'_, SharedVideoEditorState>,
) -> Result<(), String> {
    let s = state.lock().await;
    s.export_cancel.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub async fn save_edit_project(project: EditProjectSave, path: String) -> Result<(), String> {
    let json = serde_json::to_string_pretty(&project)
        .map_err(|e| format!("Serialize failed: {}", e))?;
    std::fs::write(&path, json).map_err(|e| format!("Write failed: {}", e))
}

#[tauri::command]
pub async fn load_edit_project(path: String) -> Result<EditProjectSave, String> {
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Read failed: {}", e))?;
    serde_json::from_str(&content).map_err(|e| format!("Parse failed: {}", e))
}

// ---- Phase 6: FFmpeg Setup ----

#[tauri::command]
pub async fn install_ffmpeg_winget() -> Result<String, String> {
    let output = tokio::process::Command::new("winget")
        .args(["install", "--id", "Gyan.FFmpeg", "--accept-source-agreements", "--accept-package-agreements"])
        .output()
        .await
        .map_err(|e| format!("Failed to run winget: {}. Try installing manually.", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() || stdout.contains("Successfully installed") || stdout.contains("already installed") {
        Ok(stdout)
    } else {
        Err(format!("winget failed: {}\n{}", stdout, stderr))
    }
}

#[tauri::command]
pub async fn browse_for_ffmpeg() -> Result<Option<String>, String> {
    let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$dlg = New-Object System.Windows.Forms.OpenFileDialog
$dlg.Filter = 'ffmpeg.exe|ffmpeg.exe|All Files|*.*'
$dlg.Title = 'Locate ffmpeg.exe'
if ($dlg.ShowDialog() -eq 'OK') { $dlg.FileName } else { '' }
"#;
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .output()
        .map_err(|e| format!("Failed to open dialog: {}", e))?;

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        Ok(None)
    } else {
        Ok(Some(path))
    }
}

#[tauri::command]
pub async fn browse_save_location(default_name: String, filter: String) -> Result<Option<String>, String> {
    let script = format!(
        r#"
Add-Type -AssemblyName System.Windows.Forms
$dlg = New-Object System.Windows.Forms.SaveFileDialog
$dlg.Filter = '{filter}'
$dlg.Title = 'Save As'
$dlg.FileName = '{name}'
if ($dlg.ShowDialog() -eq 'OK') {{ $dlg.FileName }} else {{ '' }}
"#,
        filter = filter.replace('\'', "''"),
        name = default_name.replace('\'', "''"),
    );
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .map_err(|e| format!("Failed to open dialog: {}", e))?;

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        Ok(None)
    } else {
        Ok(Some(path))
    }
}

#[tauri::command]
pub async fn set_ffmpeg_path(
    state: tauri::State<'_, SharedVideoEditorState>,
    path: String,
) -> Result<FfmpegStatus, String> {
    let ffmpeg = PathBuf::from(&path);
    if !ffmpeg.exists() {
        return Err(format!("File not found: {}", path));
    }

    let ffprobe = ffmpeg
        .parent()
        .map(|d| d.join("ffprobe.exe"))
        .filter(|p| p.exists());

    let mut s = state.lock().await;
    s.ffmpeg_path = Some(ffmpeg.clone());
    s.ffprobe_path = ffprobe.clone();

    Ok(FfmpegStatus {
        found: true,
        path: Some(ffmpeg.to_string_lossy().to_string()),
        ffprobe_path: ffprobe.map(|p| p.to_string_lossy().to_string()),
    })
}
