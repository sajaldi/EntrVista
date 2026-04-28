use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest};
use tokio_tungstenite::tungstenite::Message;
use futures_util::{StreamExt, SinkExt};
use serde::Deserialize;
use anyhow::{Result, Context};
use std::sync::mpsc::Receiver;
use url::Url;

#[derive(Debug, Deserialize)]
pub struct DeepgramResponse {
    pub channel: DeepgramChannel,
    pub is_final: bool,
    pub speech_final: bool,
}

#[derive(Debug, Deserialize)]
pub struct DeepgramChannel {
    pub alternatives: Vec<DeepgramAlternative>,
}

#[derive(Debug, Deserialize)]
pub struct DeepgramAlternative {
    pub transcript: String,
    pub confidence: f64,
}

pub async fn process_transcription(
    api_key: &str,
    mut audio_rx: Receiver<Vec<i16>>,
    text_tx: tokio::sync::mpsc::Sender<String>,
    sample_rate: u32,
    channels: u16,
) -> Result<()> {
    let url_str = format!(
        "wss://api.deepgram.com/v1/listen?encoding=linear16&sample_rate={}&channels={}&interim_results=true&smart_format=true",
        sample_rate, channels
    );
    let url = Url::parse(&url_str)?;
    
    let mut request = url.into_client_request()?;
    request.headers_mut().insert(
        "Authorization",
        format!("Token {}", api_key).parse()?
    );

    let (ws_stream, response) = match connect_async(request).await {
        Ok(val) => val,
        Err(e) => {
            return Err(anyhow::anyhow!("Deepgram Connection Error: {}. Ensure your API Key is valid and has credits.", e));
        }
    };
    
    // Status 101 is "Switching Protocols", which is normal for WebSockets
    if !response.status().is_success() && response.status() != 101 {
        return Err(anyhow::anyhow!("Deepgram rejected connection with status: {}", response.status()));
    }
    
    let (mut write, mut read) = ws_stream.split();

    println!("Deepgram: WebSocket connection established.");

    // spawn a task to pipe audio from mpsc to websocket
    let mut write_clone = write;
    let audio_task = tokio::spawn(async move {
        while let Ok(pcm) = audio_rx.recv() {
            let bytes: Vec<u8> = pcm.iter().flat_map(|&s| s.to_le_bytes()).collect();
            if let Err(_) = write_clone.send(Message::Binary(bytes)).await {
                break;
            }
        }
        let _ = write_clone.close().await;
    });

    // Optional: KeepAlive task every 10 seconds (Deepgram requirement for long periods of silence)
    // Actually, we'll let it be for now as audio usually flows.

    // Listen for transcription results
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Ok(resp) = serde_json::from_str::<DeepgramResponse>(&text) {
                    let transcript = &resp.channel.alternatives[0].transcript;
                    if transcript.is_empty() { continue; }

                    if resp.is_final {
                        // Final — send for full processing (question detection, etc.)
                        if let Err(e) = text_tx.send(transcript.clone()).await {
                            eprintln!("Failed to send transcript: {}", e);
                            break;
                        }
                    } else {
                        // Interim — send with a tag so main.rs can just display it without AI processing
                        let _ = text_tx.send(format!("__interim__:{}", transcript)).await;
                    }
                }
            }
            Ok(Message::Close(_)) => break,
            Err(e) => {
                eprintln!("Deepgram Receiver Error: {}", e);
                break;
            }
            _ => {}
        }
    }

    let _ = audio_task.await;
    Ok(())
}
