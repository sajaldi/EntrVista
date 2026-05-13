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
use serde::Serialize;

struct AppState {
    brain: Arc<RwLock<groq::GroqBrain>>,
    language: Arc<RwLock<String>>,
    lang_tx: tokio::sync::watch::Sender<String>,
}

#[derive(Serialize)]
struct PredefinedData {
    responses: Vec<db::PredefinedItem>,
    lifesavers: Vec<db::PredefinedItem>,
    facts: Vec<db::PredefinedItem>,
}

#[tauri::command]
async fn ask_zai_specifically(text: String, state: State<'_, AppState>) -> Result<String, String> {
    println!("🚀 Manual request: {}", text);
    let mut db = db::Database::new().map_err(|e| e.to_string())?;
    let mut relevant = db.query_experiences(&text, 3).unwrap_or_default();
    
    // Inject predefined content and facts as context
    if let Ok(responses) = db.get_responses() {
        for resp in responses {
            relevant.push(format!("PREDEFINED SCRIPT (Topic: {}): {}", resp.title, resp.content));
        }
    }
    if let Ok(facts) = db.get_facts() {
        for fact in facts {
            relevant.push(format!("CORE FACT ({}): {}", fact.title, fact.content));
        }
    }
    
    let mut brain = state.brain.write().await;
    brain.ask(&text, relevant).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn generate_options(question: String, state: State<'_, AppState>) -> Result<Vec<String>, String> {
    println!("💡 Generating options for: {}", question);
    let mut db = db::Database::new().map_err(|e| e.to_string())?;
    let mut relevant = db.query_experiences(&question, 3).unwrap_or_default();

    // Inject predefined content and facts as context
    if let Ok(responses) = db.get_responses() {
        for resp in responses {
            relevant.push(format!("PREDEFINED SCRIPT (Topic: {}): {}", resp.title, resp.content));
        }
    }
    if let Ok(facts) = db.get_facts() {
        for fact in facts {
            relevant.push(format!("CORE FACT ({}): {}", fact.title, fact.content));
        }
    }

    let brain = state.brain.read().await;
    brain.generate_options(&question, &relevant).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn clear_session(state: State<'_, AppState>) -> Result<(), String> {
    let mut brain = state.brain.write().await;
    brain.clear_session();
    Ok(())
}
#[tauri::command]
async fn set_language(lang: String, state: State<'_, AppState>) -> Result<(), String> {
    println!("🌐 Switching language to: {}", lang);
    let mut current_lang = state.language.write().await;
    *current_lang = lang.clone();
    let _ = state.lang_tx.send(lang);
    Ok(())
}

#[tauri::command]
async fn get_all_predefined() -> Result<PredefinedData, String> {
    let db = db::Database::new().map_err(|e| e.to_string())?;
    let responses = db.get_responses().map_err(|e| e.to_string())?;
    let lifesavers = db.get_lifesavers().map_err(|e| e.to_string())?;
    let facts = db.get_facts().map_err(|e| e.to_string())?;
    Ok(PredefinedData { responses, lifesavers, facts })
}

#[tauri::command]
async fn save_predefined_item(item_type: String, title: String, content: String) -> Result<(), String> {
    let db = db::Database::new().map_err(|e| e.to_string())?;
    if item_type == "response" {
        db.save_response(&title, &content).map_err(|e| e.to_string())?;
    } else if item_type == "lifesaver" {
        db.save_lifesaver(&title, &content).map_err(|e| e.to_string())?;
    } else {
        db.save_fact(&title, &content).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn delete_predefined_item(item_type: String, id: i64) -> Result<(), String> {
    let db = db::Database::new().map_err(|e| e.to_string())?;
    if item_type == "response" {
        db.delete_response(id).map_err(|e| e.to_string())?;
    } else if item_type == "lifesaver" {
        db.delete_lifesaver(id).map_err(|e| e.to_string())?;
    } else {
        db.delete_fact(id).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    
    let (lang_tx, mut lang_rx) = tokio::sync::watch::channel("en".to_string());
    let language = Arc::new(RwLock::new("en".to_string()));

    tauri::Builder::default()
        .setup(move |app| {
            let handle = app.handle();
            let groq_api_key = env::var("GROQ_API_KEY").expect("GROQ_API_KEY must be set");
            let groq_api_key_2 = env::var("GROQ_API_KEY_2").unwrap_or_else(|_| groq_api_key.clone());
            let openrouter_key = env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY must be set");
            let google_key = env::var("GOOGLE_API_KEY").unwrap_or_default();

            let groq_keys = vec![groq_api_key, groq_api_key_2];
            let brain = Arc::new(RwLock::new(groq::GroqBrain::new(groq_keys, openrouter_key, google_key)));
            let brain_clone = brain.clone();
            
            // Initial DB population if needed
            {
                let db = db::Database::new().expect("DB init failed");
                if db.get_responses().unwrap_or_default().is_empty() {
                    println!("🌱 Populating initial responses...");
                    let _ = db.save_response("Tell me about yourself", "I am an Electrical Industrial Engineer with over 12 years of experience leading maintenance and reliability. Currently, as the Maintenance & Operations Manager at the CCG, I manage a Public-Private Partnership complex of over 200,000 square meters, spanning five buildings—including two 24-story towers and the largest Data Center in Honduras. But what truly defines my profile is my deep passion for continuous improvement using whatever tools I have available. My mindset has been process-oriented since my very first internship. Later, at Unilever, I was heavily trained and ingrained in a strict culture of Industrial Safety, which remains my number one priority today. Throughout my career, I’ve developed my leadership skills by always looking for opportunities to optimize operations, especially when resources are limited. A perfect example of this is my current role. When I arrived, the facility relied entirely on manual Excel spreadsheets. Using that mindset of doing more with less, I took the initiative to build a custom CMMS from scratch using Django, PostgreSQL, and AI-driven RAG tools. By doing this, I didn’t just digitize processes; I used my leadership to change the team's culture, moving them from reactive firefighting to data-driven reliability. I am here because I want to bring this process-oriented, safety-first, and resourceful mindset to a global operation like yours.");
                    let _ = db.save_response("Why do you want to work here?", "To be honest, my fascination with Amazon actually started over a decade ago. I remember the first time I ordered something back in 2013, and as an engineer, I was absolutely struck by the surgical precision of the logistics. Seeing a package being tracked in real-time from a vendor thousands of miles away right to my doorstep in Honduras was eye-opening. Since then, I’ve closely followed how Amazon stays at the absolute forefront of technology, from the integration of sophisticated robotics in fulfillment centers to the ambition of Prime Air drones.\nMy passion for this field stems from a constant drive to automate and optimize.Throughout my 15 years in maintenance, I’ve always been a builder at heart.I’ve spent a lot of time working directly with PLCs and Arduinos to create custom automation solutions when off- the - shelf tools simply couldn't meet the facility's needs.That’s actually what led me to develop my own AI - driven CMMS from scratch using Django and PostgreSQL; I needed a system that could handle high - speed data and bridge the gap between hardware automation and software reliability.\nThat’s exactly why I want to be here.I deeply share your Leadership Principles, especially the Bias for Action and the drive to Invent and Simplify.I am looking for an environment where innovative ideas aren't just discussed but are materialized at an accelerated pace. I am confident that my hybrid background—combining large-scale industrial leadership with hands-on experience in PLCs, Arduinos, and software development—fits perfectly with the culture of excellence that defines Amazon RME.");
                    let _ = db.save_response("What are your strengths and weaknesses?", "My greatest strength is my hybrid profile: I have a deep industrial background and a strict safety culture, but I also know how to code and build AI solutions to solve operational problems. As for a weakness, because I am so passionate about technology and automation, I sometimes want to digitize everything immediately. I’ve had to learn to step back, be patient, and remember that technology is only as good as the team using it. I now focus heavily on training my technicians first, so they feel comfortable and empowered by the new tools instead of feeling overwhelmed.");
                    let _ = db.save_response("Tell me about a time you showed leadership", "When I joined the CCG, the maintenance team was stuck in a reactive firefighting mode. Everything was tracked on manual Excel spreadsheets, which was inefficient and frustrating for them. Leadership for me wasn't just about giving them a new software; it was about changing the culture. I built a custom CMMS and an RFID system, but my real leadership moment was taking the time to train the technicians on the floor. I showed them how this technology would make their jobs safer and easier. By doing this, I empowered them and moved the team from a reactive mindset to data-driven reliability.");
                    let _ = db.save_response("Tell me about a difficult challenge at work", "At my current role, inventory inaccuracy was causing downtime, and diagnosing complex equipment issues took too long because technicians had to manually search through thousands of pages of manuals. We didn't have the budget for expensive enterprise software, so I took this as a challenge to 'do more with less'. I leveraged my programming skills to build a custom Django CMMS and integrated an AI-driven RAG system with Vector Databases. This allowed technicians to query machine manuals in natural language, slashing diagnostic times drastically and solving a massive operational bottleneck using just innovation and resourcefulness.");
                    let _ = db.save_response("Tell me about a time you went above and beyond your role (Ownership / Earn Trust)", "In my current role, our complex houses the largest Data Center in Honduras, which hosts all the government servers. Initially, the roles between departments were a bit ambiguous, and the IT Manager was handling everything inside the Data Center, including the physical facility maintenance. One day, the Data Center suffered a massive failure in one of its main UPS units. Instead of saying 'that's not my department', I took full Ownership. With my electrical engineering background, I immediately stepped in to support him, providing a safe technical alternative that significantly minimized the downtime of those critical servers. But I didn't stop at the emergency fix. To ensure this wouldn't happen again, I worked closely with him to earn his trust and collaboratively developed a comprehensive preventive and predictive maintenance plan for the entire Data Center. I also led the reactivation of the DCIM—the infrastructure monitoring system. By acting quickly and bridging the gap between Facilities and IT, we protected the country's most critical infrastructure and built a proactive, data-driven operation.");
                }
                if db.get_lifesavers().unwrap_or_default().is_empty() {
                    println!("🌱 Populating initial lifesavers...");
                    let _ = db.save_lifesaver("Lifesaver 1", "That’s a great question. Let me structure my thoughts for a second to give you the most accurate answer...");
                }

                // NEW: Index contex.md if it exists and database is empty
                if let Ok(content) = std::fs::read_to_string("contex.md") {
                    // Simple check: if we have very few experiences, assume we need to index
                    // (Using query to check count would be better but let's check meta table)
                    let mut db_mut = db::Database::new().expect("DB init failed");
                    let count: i64 = db_mut.get_experience_count().unwrap_or(0);
                    if count == 0 {
                        println!("📖 contex.md found and DB is empty. Indexing...");
                        let chunks: Vec<&str> = content.split("\n\n").filter(|s| !s.trim().is_empty()).collect();
                        for chunk in &chunks {
                            let _ = db_mut.add_experience(chunk.trim());
                        }
                        println!("✅ Indexed {} chunks from contex.md", chunks.len());
                    }
                }
            }

            app.manage(AppState { 
                brain: brain.clone(),
                language: language.clone(),
                lang_tx,
            });

            tokio::spawn(async move {
                let deepgram_api_key = env::var("DEEPGRAM_API_KEY").expect("DEEPGRAM_API_KEY must be set");
                let mut database = db::Database::new().expect("DB init failed");
                let (audio_tx, audio_rx) = mpsc::channel::<Vec<i16>>();
                let (text_tx, mut text_rx) = tokio_mpsc::channel::<String>(100);

                // Audio Capture (Run once)
                let (sample_rate, channels) = audio::start_audio_capture(audio_tx).expect("Audio init failed");
                let audio_rx = Arc::new(std::sync::Mutex::new(audio_rx));

                // Transcription loop that can restart on language change
                let text_tx_clone = text_tx.clone();
                let deepgram_key = deepgram_api_key.clone();
                
                tokio::spawn(async move {
                    loop {
                        let current_lang = lang_rx.borrow().clone();
                        println!("🎙️ Starting Deepgram session with language: {}", current_lang);
                        
                        let (session_audio_tx, session_audio_rx) = mpsc::channel();
                        let audio_rx_inner = audio_rx.clone();
                        
                        // Proxy task to forward audio to the current session
                        let proxy_handle = tokio::task::spawn_blocking(move || {
                            while let Ok(data) = audio_rx_inner.lock().unwrap().recv() {
                                if session_audio_tx.send(data).is_err() { break; }
                            }
                        });

                        let session_text_tx = text_tx_clone.clone();
                        let lang = current_lang.clone();
                        let dg_key = deepgram_key.clone();
                        
                        let transcription_fut = deepgram::process_transcription(
                            &dg_key, 
                            session_audio_rx, 
                            session_text_tx, 
                            sample_rate, 
                            channels,
                            &lang
                        );

                        tokio::select! {
                            res = transcription_fut => {
                                if let Err(e) = res {
                                    eprintln!("❌ Deepgram Session Error: {}", e);
                                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                                }
                            }
                            _ = lang_rx.changed() => {
                                println!("🔄 Language change detected, restarting transcription...");
                            }
                        }
                        
                        // proxy_handle will naturally terminate when session_audio_rx is dropped
                        drop(proxy_handle); 
                    }
                });

                // Transcript processing loop
                while let Some(transcript) = text_rx.recv().await {
                    if let Some(interim_text) = transcript.strip_prefix("__interim__:") {
                        let _ = handle.emit_all("new-interim", interim_text);
                        continue;
                    }

                    {
                        let mut brain_write = brain_clone.write().await;
                        brain_write.update_context(&transcript);
                        let _ = handle.emit_all("context-switch", brain_write.get_model_name());
                    }

                    let _ = handle.emit_all("new-transcript", &transcript);

                    if transcript.trim().ends_with('?') {
                        let relevant = database.query_experiences(&transcript, 3).unwrap_or_default();
                        let brain_read = brain_clone.read().await;
                        if let Ok(options) = brain_read.generate_options(&transcript, &relevant).await {
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
        .invoke_handler(tauri::generate_handler![
            ask_zai_specifically, 
            generate_options, 
            set_language, 
            get_all_predefined, 
            save_predefined_item,
            delete_predefined_item,
            clear_session
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    Ok(())
}

