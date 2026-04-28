mod audio;
mod deepgram;
mod groq;
mod db;

use std::sync::mpsc;
use tokio::sync::mpsc as tokio_mpsc;
use dotenv::dotenv;
use std::env;
use anyhow::Result;

use tauri::{Manager, State};
use std::sync::Arc;
use tokio::sync::RwLock;

struct AppState {
    brain: Arc<RwLock<groq::GroqBrain>>,
}

#[tauri::command]
async fn ask_zai_specifically(text: String, state: State<'_, AppState>) -> Result<String, String> {
    println!("🚀 Manual request: {}", text);
    let brain = state.brain.read().await;
    brain.ask(&text, vec![]).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn generate_options(question: String, state: State<'_, AppState>) -> Result<Vec<String>, String> {
    println!("💡 Generating options for: {}", question);
    let brain = state.brain.read().await;
    brain.generate_options(&question).await.map_err(|e| e.to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle();
            let deepgram_api_key = env::var("DEEPGRAM_API_KEY").expect("DEEPGRAM_API_KEY must be set");
            let groq_api_key = env::var("GROQ_API_KEY").expect("GROQ_API_KEY must be set");
            let openrouter_key = env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY must be set");
            let google_key = env::var("GOOGLE_API_KEY").unwrap_or_default();

            let brain = Arc::new(RwLock::new(groq::GroqBrain::new(groq_api_key, openrouter_key, google_key)));
            let brain_clone = brain.clone();
            
            app.manage(AppState { brain: brain.clone() });

            tokio::spawn(async move {
                let mut database = db::Database::new().expect("DB init failed");
                let (audio_tx, audio_rx) = mpsc::channel::<Vec<i16>>();
                let (text_tx, mut text_rx) = tokio_mpsc::channel::<String>(100);

                // Audio Loopback
                let (sample_rate, channels) = audio::start_audio_capture(audio_tx).expect("Audio init failed");

                tokio::spawn(async move {
                    if let Err(e) = deepgram::process_transcription(&deepgram_api_key, audio_rx, text_tx, sample_rate, channels).await {
                        eprintln!("❌ Deepgram Task Error: {}", e);
                    }
                });

                while let Some(transcript) = text_rx.recv().await {
                    // Interim results: just display them, no AI processing
                    if let Some(interim_text) = transcript.strip_prefix("__interim__:") {
                        let _ = handle.emit_all("new-interim", interim_text);
                        continue;
                    }

                    // Final transcript: update context badge and emit to UI
                    {
                        let mut brain_write = brain_clone.write().await;
                        brain_write.update_context(&transcript);
                        let name = brain_write.get_model_name().to_string();
                        let _ = handle.emit_all("context-switch", name);
                    }

                    let _ = handle.emit_all("new-transcript", &transcript);

                    let is_question = transcript.trim().ends_with('?') ||
                                     transcript.to_lowercase().starts_with("qué");

                    if is_question {
                        // Generate options for user to pick from
                        let brain_read = brain_clone.read().await;
                        if let Ok(options) = brain_read.generate_options(&transcript).await {
                            let _ = handle.emit_all("new-options", serde_json::json!({
                                "question": &transcript,
                                "options": options
                            }));
                        }
                    } else if transcript.split_whitespace().count() > 4 {
                        let _ = database.add_experience(&transcript);
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![ask_zai_specifically, generate_options])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    Ok(())
}
