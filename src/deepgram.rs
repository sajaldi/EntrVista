use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest};
use tokio_tungstenite::tungstenite::Message;
use futures_util::{StreamExt, SinkExt};
use serde::Deserialize;
use anyhow::{Result, Context};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use url::Url;
use std::time::Duration;

#[derive(Debug, Deserialize)]
pub struct DeepgramResponse {
    pub channel: DeepgramChannel,
    #[serde(default)]
    pub is_final: bool,
    #[serde(default)]
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
    audio_rx: Receiver<Vec<i16>>,
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
            return Err(anyhow::anyhow!("Deepgram Connection Error: {}. Ensure your API Key is valid.", e));
        }
    };
    
    if !response.status().is_success() && response.status() != 101 {
        return Err(anyhow::anyhow!("Deepgram rejected connection with status: {}", response.status()));
    }
    
    let (mut write, mut read) = ws_stream.split();
    println!("Deepgram: WebSocket connection established.");

    let audio_rx = Arc::new(std::sync::Mutex::new(audio_rx));

    // Task 1: Sender (Audio + KeepAlive)
    let audio_task = tokio::spawn(async move {
        let mut last_keep_alive = std::time::Instant::now();
        
        loop {
            // Receive audio with timeout to allow KeepAlive checks
            let audio_rx_clone = Arc::clone(&audio_rx);
            let pcm_result = tokio::task::spawn_blocking(move || {
                audio_rx_clone.lock().unwrap().recv_timeout(Duration::from_millis(100))
            }).await.unwrap();

            // Send audio if we have it
            if let Ok(data) = pcm_result {
                let bytes: Vec<u8> = data.iter().flat_map(|&s| s.to_le_bytes()).collect();
                if let Err(_) = write.send(Message::Binary(bytes)).await { break; }
            } else if let Err(std::sync::mpsc::RecvTimeoutError::Disconnected) = pcm_result {
                break;
            }

            // Send KeepAlive every 10 seconds
            if last_keep_alive.elapsed().as_secs() >= 10 {
                if let Err(_) = write.send(Message::Text("{\"type\": \"KeepAlive\"}".to_string())).await {
                    break;
                }
                last_keep_alive = std::time::Instant::now();
            }
        }
        let _ = write.close().await;
    });

    // Task 2: Receiver
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Ok(resp) = serde_json::from_str::<DeepgramResponse>(&text) {
                    let transcript = &resp.channel.alternatives[0].transcript;
                    if transcript.is_empty() { continue; }

                    if resp.is_final {
                        println!("✅ Final Transcript: {}", transcript);
                        let _ = text_tx.send(transcript.clone()).await;
                    } else {
                        let _ = text_tx.send(format!("__interim__:{}", transcript)).await;
                    }
                }
            }
            Ok(Message::Close(_)) => {
                println!("Deepgram: Connection closed by server.");
                break;
            }
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
