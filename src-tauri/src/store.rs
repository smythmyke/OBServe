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
struct StoredLicense {
    key: String,
    email: Option<String>,
    modules: Vec<String>,
    activated_at: u64,
}

pub fn get_module_catalog() -> Vec<ModuleInfo> {
    vec![
        ModuleInfo {
            id: "spectrum".into(),
            name: "Pro Spectrum".into(),
            description: "Live FFT analyzer, LUFS metering, and processing knobs".into(),
            price_cents: 199,
            stripe_link: "https://buy.stripe.com/4gMcN46Vc8YkaLQ8Yy0co08".into(),
            panels: vec!["pro-spectrum".into()],
        },
        ModuleInfo {
            id: "video-editor".into(),
            name: "Video Review & Editor".into(),
            description: "Trim, split, overlay, and export recordings with FFmpeg".into(),
            price_cents: 199,
            stripe_link: "https://buy.stripe.com/00wcN4djA6QcbPU2Aa0co09".into(),
            panels: vec!["video-editor".into()],
        },
        ModuleInfo {
            id: "calibration".into(),
            name: "Audio Calibration".into(),
            description: "Guided mic calibration wizard with spectral analysis".into(),
            price_cents: 199,
            stripe_link: "https://buy.stripe.com/6oU00i1ASfmI7zE8Yy0co0a".into(),
            panels: vec!["calibration".into()],
        },
        ModuleInfo {
            id: "ducking".into(),
            name: "Sidechain Ducking + Mixer".into(),
            description: "Auto-duck music when speaking, full audio mixer controls".into(),
            price_cents: 199,
            stripe_link: "https://buy.stripe.com/cNicN44N4eiE7zEa2C0co0b".into(),
            panels: vec!["ducking".into(), "mixer".into()],
        },
        ModuleInfo {
            id: "audio-fx".into(),
            name: "Airwindows VST Plugins".into(),
            description: "16 bundled Airwindows VST2 plugins for professional audio processing".into(),
            price_cents: 199,
            stripe_link: "https://buy.stripe.com/bJe8wO2EWb6sbPU2Aa0co0c".into(),
            panels: vec![],
        },
        ModuleInfo {
            id: "camera".into(),
            name: "Camera Scene Auto-Detect".into(),
            description: "Automatically detect cameras and create OBS scenes".into(),
            price_cents: 199,
            stripe_link: "https://buy.stripe.com/28E3cu7Zg0rOg6agr00co0d".into(),
            panels: vec!["webcam".into()],
        },
        ModuleInfo {
            id: "presets".into(),
            name: "Smart Presets".into(),
            description: "One-click audio presets for tutorial, gaming, podcast, and more".into(),
            price_cents: 199,
            stripe_link: "https://buy.stripe.com/28E9AS1AS3E0f263Ee0co0e".into(),
            panels: vec![],
        },
        ModuleInfo {
            id: "narration-studio".into(),
            name: "Narration Studio".into(),
            description: "High-quality post-filter narration capture via VB-Cable".into(),
            price_cents: 199,
            stripe_link: "https://buy.stripe.com/PLACEHOLDER".into(),
            panels: vec![],
        },
        ModuleInfo {
            id: "monitoring".into(),
            name: "Advanced Monitoring".into(),
            description: "Enhanced system monitoring with GPU stats and alerts".into(),
            price_cents: 199,
            stripe_link: "https://buy.stripe.com/8x28wO1ASgqMg6ab6G0co0f".into(),
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
            Ok(stored) => LicenseState {
                owned_modules: stored.modules.into_iter().collect(),
                email: stored.email,
                activated_at: Some(stored.activated_at),
            },
            Err(e) => {
                log::warn!("Failed to parse license file: {}", e);
                LicenseState::default()
            }
        },
        Err(_) => LicenseState::default(),
    }
}

fn save_license(state: &LicenseState, key: &str) -> Result<(), String> {
    let dir = license_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create dir: {}", e))?;
    let path = license_file_path();
    let stored = StoredLicense {
        key: key.to_string(),
        email: state.email.clone(),
        modules: state.owned_modules.iter().cloned().collect(),
        activated_at: state.activated_at.unwrap_or(0),
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
    save_license(&new_state, &key)?;

    let mut state = license.write().await;
    *state = new_state.clone();

    Ok(new_state)
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
