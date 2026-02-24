use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

pub type SharedLicenseState = Arc<RwLock<LicenseState>>;

const LICENSE_PUBLIC_KEY_B64: &str = "VLnMNE9WY3KsKicAniGG/hCSE4GzwYNSd21K9PVya6w=";

const B64_URL: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::URL_SAFE_NO_PAD;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub price_cents: u32,
    pub stripe_link: String,
    pub panels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseState {
    pub owned_modules: HashSet<String>,
    pub email: Option<String>,
    pub activated_at: Option<u64>,
}

impl Default for LicenseState {
    fn default() -> Self {
        Self {
            owned_modules: HashSet::new(),
            email: None,
            activated_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LicensePayload {
    modules: Vec<String>,
    email: String,
    ts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredLicenseKey {
    key: String,
    email: Option<String>,
    modules: Vec<String>,
    activated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredLicense {
    #[serde(default)]
    keys: Vec<StoredLicenseKey>,
    // Legacy single-key fields for backward compatibility on load
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    modules: Option<Vec<String>>,
    #[serde(default)]
    activated_at: Option<u64>,
}

pub fn get_module_catalog() -> Vec<ModuleInfo> {
    vec![
        ModuleInfo {
            id: "spectrum".into(),
            name: "Pro Spectrum".into(),
            description: "Live FFT analyzer, LUFS metering, and processing knobs".into(),
            price_cents: 499,
            stripe_link: "https://buy.stripe.com/4gM4gy2EWcaw3jofmW0co0g".into(),
            panels: vec!["pro-spectrum".into()],
        },
        ModuleInfo {
            id: "video-editor".into(),
            name: "Video Review & Editor".into(),
            description: "Trim, split, overlay, and export recordings with FFmpeg".into(),
            price_cents: 499,
            stripe_link: "https://buy.stripe.com/6oUeVc5R80rOcTYeiS0co0h".into(),
            panels: vec!["video-editor".into()],
        },
        ModuleInfo {
            id: "calibration".into(),
            name: "Audio Calibration".into(),
            description: "Guided mic calibration wizard with spectral analysis".into(),
            price_cents: 499,
            stripe_link: "https://buy.stripe.com/bJeeVc7Zg6QcbPU6Qq0co0i".into(),
            panels: vec!["calibration".into()],
        },
        ModuleInfo {
            id: "ducking".into(),
            name: "Sidechain Ducking + Mixer".into(),
            description: "Auto-duck music when speaking, full audio mixer controls".into(),
            price_cents: 499,
            stripe_link: "https://buy.stripe.com/6oU6oGfrIgqM8DI5Mm0co0j".into(),
            panels: vec!["ducking".into(), "mixer".into()],
        },
        ModuleInfo {
            id: "audio-fx".into(),
            name: "Airwindows VST Plugins".into(),
            description: "16 bundled Airwindows VST2 plugins for professional audio processing".into(),
            price_cents: 499,
            stripe_link: "https://buy.stripe.com/7sY3cufrI2zW1bg3Ee0co0k".into(),
            panels: vec![],
        },
        ModuleInfo {
            id: "camera".into(),
            name: "Camera Scene Auto-Detect".into(),
            description: "Automatically detect cameras and create OBS scenes".into(),
            price_cents: 499,
            stripe_link: "https://buy.stripe.com/28E7sKcfw0rOaLQ3Ee0co0l".into(),
            panels: vec!["webcam".into()],
        },
        ModuleInfo {
            id: "presets".into(),
            name: "Smart Presets".into(),
            description: "One-click audio presets for tutorial, gaming, podcast, and more".into(),
            price_cents: 499,
            stripe_link: "https://buy.stripe.com/4gM8wO1AS3E01bg7Uu0co0m".into(),
            panels: vec![],
        },
        ModuleInfo {
            id: "narration-studio".into(),
            name: "Narration Studio".into(),
            description: "High-quality post-filter narration capture via VB-Cable".into(),
            price_cents: 499,
            stripe_link: "https://buy.stripe.com/8x29ASenEcawcTY1w60co0n".into(),
            panels: vec![],
        },
        ModuleInfo {
            id: "monitoring".into(),
            name: "Advanced Monitoring".into(),
            description: "Enhanced system monitoring with GPU stats and alerts".into(),
            price_cents: 499,
            stripe_link: "https://buy.stripe.com/aFacN4djA3E007cdeO0co0o".into(),
            panels: vec![],
        },
        ModuleInfo {
            id: "sample-pad".into(),
            name: "OBServe Pads".into(),
            description: "MPC-style sample pad — load, trigger, and mix audio clips live".into(),
            price_cents: 499,
            stripe_link: "https://buy.stripe.com/4gMfZgcfw0rO2fk2Aa0co0p".into(),
            panels: vec!["pads".into()],
        },
        ModuleInfo {
            id: "all-modules-bundle".into(),
            name: "All Modules Bundle".into(),
            description: "Unlock every OBServe module — best value!".into(),
            price_cents: 2999,
            stripe_link: "https://buy.stripe.com/28E6oG1AS1vSaLQa2C0co0q".into(),
            panels: vec![],
        },
    ]
}

fn license_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.observe.app")
}

fn license_file_path() -> PathBuf {
    license_dir().join("license.json")
}

pub fn load_license_from_disk() -> LicenseState {
    let path = license_file_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<StoredLicense>(&content) {
            Ok(stored) => {
                let mut modules = HashSet::new();
                let mut email = None;
                let mut latest_ts: u64 = 0;

                // Load from multi-key format
                for k in &stored.keys {
                    modules.extend(k.modules.iter().cloned());
                    if k.activated_at > latest_ts {
                        latest_ts = k.activated_at;
                        email = k.email.clone();
                    }
                }

                // Backward compat: load legacy single-key fields
                if stored.keys.is_empty() {
                    if let Some(legacy_modules) = stored.modules {
                        modules.extend(legacy_modules);
                    }
                    if let Some(ts) = stored.activated_at {
                        latest_ts = ts;
                    }
                    email = stored.email;
                }

                LicenseState {
                    owned_modules: modules,
                    email,
                    activated_at: if latest_ts > 0 { Some(latest_ts) } else { None },
                }
            }
            Err(e) => {
                log::warn!("Failed to parse license file: {}", e);
                LicenseState::default()
            }
        },
        Err(_) => LicenseState::default(),
    }
}

fn save_license(new_key: &str, new_email: Option<&str>, new_modules: &[String], new_ts: u64) -> Result<(), String> {
    let dir = license_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create dir: {}", e))?;
    let path = license_file_path();

    // Load existing keys to preserve them
    let mut keys: Vec<StoredLicenseKey> = if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(existing) = serde_json::from_str::<StoredLicense>(&content) {
            let mut k = existing.keys;
            // Migrate legacy single-key format
            if k.is_empty() {
                if let Some(legacy_key) = existing.key {
                    k.push(StoredLicenseKey {
                        key: legacy_key,
                        email: existing.email,
                        modules: existing.modules.unwrap_or_default(),
                        activated_at: existing.activated_at.unwrap_or(0),
                    });
                }
            }
            k
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // Replace if same key exists, otherwise append
    let existing_idx = keys.iter().position(|k| k.key == new_key);
    let entry = StoredLicenseKey {
        key: new_key.to_string(),
        email: new_email.map(String::from),
        modules: new_modules.to_vec(),
        activated_at: new_ts,
    };
    if let Some(idx) = existing_idx {
        keys[idx] = entry;
    } else {
        keys.push(entry);
    }

    let stored = StoredLicense {
        keys,
        key: None,
        email: None,
        modules: None,
        activated_at: None,
    };
    let json = serde_json::to_string_pretty(&stored)
        .map_err(|e| format!("Failed to serialize license: {}", e))?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to write license: {}", e))?;
    Ok(())
}

pub fn activate_license(key: &str) -> Result<LicenseState, String> {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.len() != 2 {
        return Err("Invalid license key format".into());
    }

    let payload_b64 = parts[0];
    let signature_b64 = parts[1];

    let payload_bytes = B64_URL
        .decode(payload_b64)
        .map_err(|_| "Invalid license key: bad payload encoding")?;
    let sig_bytes = B64_URL
        .decode(signature_b64)
        .map_err(|_| "Invalid license key: bad signature encoding")?;

    let pub_key_bytes = base64::engine::general_purpose::STANDARD
        .decode(LICENSE_PUBLIC_KEY_B64)
        .map_err(|_| "Internal error: invalid public key")?;

    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    let verifying_key = VerifyingKey::from_bytes(
        pub_key_bytes
            .as_slice()
            .try_into()
            .map_err(|_| "Internal error: invalid public key length")?,
    )
    .map_err(|_| "Internal error: invalid public key")?;

    let signature = Signature::from_bytes(
        sig_bytes
            .as_slice()
            .try_into()
            .map_err(|_| "Invalid license key: bad signature length")?,
    );

    verifying_key
        .verify(&payload_bytes, &signature)
        .map_err(|_| "Invalid license key: signature verification failed")?;

    let payload: LicensePayload = serde_json::from_slice(&payload_bytes)
        .map_err(|_| "Invalid license key: bad payload")?;

    Ok(LicenseState {
        owned_modules: payload.modules.into_iter().collect(),
        email: Some(payload.email),
        activated_at: Some(payload.ts),
    })
}

#[allow(dead_code)]
pub fn is_module_owned(state: &LicenseState, module_id: &str) -> bool {
    state.owned_modules.contains(module_id)
}

pub async fn require_module(
    license: &SharedLicenseState,
    module_id: &str,
) -> Result<(), String> {
    let state = license.read().await;
    if state.owned_modules.contains(module_id) {
        Ok(())
    } else {
        let catalog = get_module_catalog();
        let name = catalog
            .iter()
            .find(|m| m.id == module_id)
            .map(|m| m.name.as_str())
            .unwrap_or(module_id);
        Err(format!("Module '{}' not purchased", name))
    }
}

// --- Device Fingerprint ---

#[cfg(target_os = "windows")]
fn read_machine_guid() -> Option<String> {
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::{
        RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ, REG_SZ,
    };

    unsafe {
        let sub_key = HSTRING::from(r"SOFTWARE\Microsoft\Cryptography");
        let mut hkey = HKEY::default();
        let result = RegOpenKeyExW(HKEY_LOCAL_MACHINE, &sub_key, 0, KEY_READ, &mut hkey);
        if result.is_err() {
            return None;
        }

        let value_name = HSTRING::from("MachineGuid");
        let mut data_type = REG_SZ;
        let mut data_size: u32 = 0;

        let result = RegQueryValueExW(
            hkey,
            &value_name,
            Some(std::ptr::null_mut()),
            Some(&mut data_type),
            None,
            Some(&mut data_size),
        );
        if result.is_err() || data_size == 0 {
            return None;
        }

        let mut buffer = vec![0u8; data_size as usize];
        let result = RegQueryValueExW(
            hkey,
            &value_name,
            Some(std::ptr::null_mut()),
            Some(&mut data_type),
            Some(buffer.as_mut_ptr()),
            Some(&mut data_size),
        );
        if result.is_err() {
            return None;
        }

        let wide: &[u16] =
            std::slice::from_raw_parts(buffer.as_ptr() as *const u16, (data_size as usize) / 2);
        let s = String::from_utf16_lossy(wide);
        let s = s.trim_end_matches('\0').to_string();
        if s.is_empty() { None } else { Some(s) }
    }
}

#[cfg(not(target_os = "windows"))]
fn read_machine_guid() -> Option<String> {
    None
}

fn get_or_create_fallback_device_id() -> Result<String, String> {
    let dir = license_dir();
    let path = dir.join("device_id");
    if let Ok(id) = std::fs::read_to_string(&path) {
        let id = id.trim().to_string();
        if !id.is_empty() {
            return Ok(id);
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create dir: {}", e))?;
    std::fs::write(&path, &id).map_err(|e| format!("Failed to write device_id: {}", e))?;
    Ok(id)
}

#[tauri::command]
pub fn get_device_fingerprint() -> Result<String, String> {
    let source = match read_machine_guid() {
        Some(guid) => guid,
        None => get_or_create_fallback_device_id()?,
    };

    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    let hash = hasher.finalize();
    Ok(hash.iter().map(|b| format!("{:02x}", b)).collect())
}

// --- Tauri Commands ---

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub price_cents: u32,
    pub stripe_link: String,
    pub panels: Vec<String>,
    pub owned: bool,
}

#[tauri::command]
pub async fn get_store_catalog(
    license: tauri::State<'_, SharedLicenseState>,
) -> Result<Vec<CatalogEntry>, String> {
    let state = license.read().await;
    let catalog = get_module_catalog();
    Ok(catalog
        .into_iter()
        .map(|m| CatalogEntry {
            owned: state.owned_modules.contains(&m.id),
            id: m.id,
            name: m.name,
            description: m.description,
            price_cents: m.price_cents,
            stripe_link: m.stripe_link,
            panels: m.panels,
        })
        .collect())
}

#[tauri::command]
pub async fn get_license_state(
    license: tauri::State<'_, SharedLicenseState>,
) -> Result<LicenseState, String> {
    let state = license.read().await;
    Ok(state.clone())
}

#[tauri::command]
pub async fn activate_license_key(
    license: tauri::State<'_, SharedLicenseState>,
    key: String,
) -> Result<LicenseState, String> {
    let new_state = activate_license(&key)?;

    // Save the new key (preserves existing keys on disk)
    let new_modules: Vec<String> = new_state.owned_modules.iter().cloned().collect();
    save_license(&key, new_state.email.as_deref(), &new_modules, new_state.activated_at.unwrap_or(0))?;

    // Merge new modules into existing in-memory state
    let mut state = license.write().await;
    state.owned_modules.extend(new_state.owned_modules);
    if new_state.email.is_some() {
        state.email = new_state.email;
    }
    if new_state.activated_at > state.activated_at {
        state.activated_at = new_state.activated_at;
    }

    Ok(state.clone())
}

#[tauri::command]
pub fn get_stored_license_keys() -> Result<Vec<String>, String> {
    let path = license_file_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<StoredLicense>(&content) {
            Ok(stored) => {
                let mut keys: Vec<String> = stored.keys.iter().map(|k| k.key.clone()).collect();
                if keys.is_empty() {
                    if let Some(legacy_key) = stored.key {
                        keys.push(legacy_key);
                    }
                }
                Ok(keys)
            }
            Err(_) => Ok(Vec::new()),
        },
        Err(_) => Ok(Vec::new()),
    }
}

#[tauri::command]
pub async fn deactivate_license(
    license: tauri::State<'_, SharedLicenseState>,
) -> Result<(), String> {
    let path = license_file_path();
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("Failed to remove license: {}", e))?;
    }

    let mut state = license.write().await;
    *state = LicenseState::default();

    Ok(())
}
