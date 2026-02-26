//! Audio commands — TTS generation and speech-to-text.
//!
//! These are convenience wrappers around route_request for audio-specific tasks.

use crate::commands::inference::TaskRouterState;
use crate::registry::types::{TaskRequest, TaskType};
use std::collections::HashMap;
use tauri::State;

/// Generate speech from text using the KittenTTS engine.
///
/// # Arguments
/// * `input` — Text to synthesize
/// * `voice` — Optional voice name (e.g. "Leo", "Bella")
/// * `speed` — Optional speaking speed (e.g. 1.0)
///
/// # Returns
/// Absolute path to the generated `.wav` file.
#[tauri::command]
pub async fn generate_speech(
    input: String,
    voice: Option<String>,
    speed: Option<f32>,
    router_state: State<'_, TaskRouterState>,
) -> Result<String, String> {
    let mut extra = HashMap::new();

    if let Some(v) = voice {
        extra.insert("voice".to_string(), v);
    }
    if let Some(s) = speed {
        extra.insert("speed".to_string(), s.to_string());
    }

    let request = TaskRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        task_type: TaskType::Tts,
        input,
        model_override: None,
        extra,
    };

    match router_state.0.route(&request).await? {
        crate::registry::types::TaskResponse::FilePath(path) => Ok(path),
        other => Err(format!("Unexpected TTS response: {other:?}")),
    }
}

/// Transcribe an audio file to text using Whisper.
///
/// # Arguments
/// * `audio_path` — Absolute path to the audio file
///
/// # Returns
/// Transcription result with timestamps.
#[tauri::command]
pub async fn transcribe_audio(
    audio_path: String,
    router_state: State<'_, TaskRouterState>,
) -> Result<crate::registry::types::TaskResponse, String> {
    let request = TaskRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        task_type: TaskType::Transcribe,
        input: audio_path,
        model_override: None,
        extra: HashMap::new(),
    };

    router_state.0.route(&request).await
}

/// Read an audio file and return it as a base64-encoded data URL.
///
/// This avoids the need for the Tauri asset protocol scope — the webview
/// can play the audio directly from the data URL.
#[tauri::command]
pub fn read_audio_base64(path: String) -> Result<String, String> {
    use base64::{engine::general_purpose::STANDARD, Engine};

    let path = std::path::Path::new(&path);
    if !path.exists() {
        return Err(format!("Audio file not found: {}", path.display()));
    }

    let mime = match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("wav") => "audio/wav",
        Some("mp3") => "audio/mpeg",
        Some("ogg") | Some("opus") => "audio/ogg",
        Some("flac") => "audio/flac",
        Some("aac") | Some("m4a") => "audio/aac",
        _ => "audio/wav",
    };

    let data = std::fs::read(path).map_err(|e| format!("Failed to read audio file: {e}"))?;
    let b64 = STANDARD.encode(&data);
    Ok(format!("data:{};base64,{}", mime, b64))
}
