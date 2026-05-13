use reqwest::Client;
use serde_json::json;
use anyhow::Result;
use std::fs;

const CHUNK_SIZE: usize = 2000; // characters per chunk

pub enum AiProvider {
    Groq,
    OpenRouter,
    Google,
}

pub struct GroqBrain {
    groq_keys: Vec<String>,
    current_key_idx: std::sync::atomic::AtomicUsize,
    openrouter_key: String,
    google_key: String,
    client: Client,
    provider: AiProvider,
    model: String,
    prompt_template: String,
    pub conversation_history: Vec<serde_json::Value>,
}

impl GroqBrain {
    pub fn new(groq_keys: Vec<String>, openrouter_key: String, google_key: String) -> Self {
        let prompt_template = Self::load_prompt_file("prompt.md");

        if prompt_template.is_empty() {
            println!("⚠️  prompt.md is empty or not found. Using default prompt.");
        } else {
            println!("🎯 Loaded master prompt from prompt.md");
        }

        Self {
            groq_keys,
            current_key_idx: std::sync::atomic::AtomicUsize::new(0),
            openrouter_key,
            google_key,
            client: Client::new(),
            provider: AiProvider::Groq,
            model: "llama-3.3-70b-versatile".to_string(),
            prompt_template,
            conversation_history: Vec::new(),
        }
    }

    /// Get the current Groq key and rotate if needed
    fn get_groq_key(&self) -> &str {
        let idx = self.current_key_idx.load(std::sync::atomic::Ordering::SeqCst);
        &self.groq_keys[idx % self.groq_keys.len()]
    }

    fn rotate_key(&self) {
        if self.groq_keys.len() > 1 {
            println!("🔄 Rate limit or error detected. Rotating to next API key...");
            self.current_key_idx.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    /// Load prompt.md as the master prompt template
    fn load_prompt_file(path: &str) -> String {
        fs::read_to_string(path).unwrap_or_default().trim().to_string()
    }

    /// Build the system prompt by injecting specific relevant context
    fn build_system_prompt(&self, relevant_experiences: &[String]) -> String {
        let personal_context = if relevant_experiences.is_empty() {
            "No specific relevant context found for this question in your experiences.".to_string()
        } else {
            relevant_experiences.join("\n\n---\n\n")
        };

        if self.prompt_template.is_empty() {
            return format!(
                "You are an elite interview coach. Always respond in English using a natural STAR narrative.\nContext:\n{}",
                personal_context
            );
        }

        // Replace context and remove the question placeholder since it goes in a user message now
        self.prompt_template
            .replace("{{CONTEXT}}", &personal_context)
            .replace("INTERVIEW QUESTION: {{QUESTION}}", "")
    }

    /// Generate 3 short answer options based on the question and relevant experiences from RAG
    pub async fn generate_options(&self, question: &str, relevant_experiences: &[String]) -> Result<Vec<String>> {
        let personal_context = if relevant_experiences.is_empty() {
            "No specific relevant context found.".to_string()
        } else {
            relevant_experiences.join("\n\n---\n\n")
        };

        let system_message = "You are an elite interview coach. Your task is to generate 3 extremely concise, highly relevant answer options based strictly on the provided context (Predefined Scripts, Facts, and Experiences). Prioritize using the Predefined Scripts if they match the topic.";
        let user_message = format!(
            "CONTEXT:\n{}\n\nQUESTION: \"{}\"\n\nGenerate 3 short, specific answer options (one line each). Each must be a different specific angle or project from the context.\n\nRespond ONLY as a numbered list:\n1. [Option A]\n2. [Option B]\n3. [Option C]",
            personal_context, question
        );

        let messages = vec![
            serde_json::json!({"role": "system", "content": system_message}),
            serde_json::json!({"role": "user", "content": user_message})
        ];

        let raw = self.call_ai_messages(messages).await?;
        
        // Parse the numbered list into Vec<String>
        let options: Vec<String> = raw
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.starts_with("1.") || trimmed.starts_with("2.") || trimmed.starts_with("3.") {
                    Some(trimmed[2..].trim().to_string())
                } else {
                    None
                }
            })
            .collect();

        if options.is_empty() {
            println!("⚠️ Parsing options failed. Raw response was:\n{}", raw);
            // Fallback: split by newlines if parsing fails
            Ok(raw.lines()
                .filter(|l| !l.trim().is_empty())
                .take(3)
                .map(|l| l.trim().to_string())
                .collect())
        } else {
            Ok(options)
        }
    }

    pub fn update_context(&mut self, _text: &str) {
        // Exclusively use Groq as requested
        self.provider = AiProvider::Groq;
        self.model = "llama-3.3-70b-versatile".to_string();
    }

    pub fn get_model_name(&self) -> &str {
        match self.provider {
            AiProvider::Groq => "GROQ",
            AiProvider::OpenRouter => "OPENROUTER",
            AiProvider::Google => "GOOGLE",
        }
    }

    pub fn clear_session(&mut self) {
        self.conversation_history.clear();
        println!("🧹 AI session history cleared.");
    }

    /// Internal: call the AI with a JSON messages array
    async fn call_ai_messages(&self, messages: Vec<serde_json::Value>) -> Result<String> {
        let max_retries = self.groq_keys.len();
        let mut last_error = None;

        for _ in 0..max_retries {
            let (url, api_key) = match self.provider {
                AiProvider::Groq => ("https://api.groq.com/openai/v1/chat/completions", self.get_groq_key()),
                AiProvider::OpenRouter => ("https://openrouter.ai/api/v1/chat/completions", self.openrouter_key.as_str()),
                AiProvider::Google => {
                    let url = format!(
                        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                        self.model, self.google_key
                    );
                    // For Google, we map the messages to their format
                    let mut contents = Vec::new();
                    for msg in &messages {
                        let role = if msg["role"] == "assistant" { "model" } else { "user" };
                        let text = msg["content"].as_str().unwrap_or("");
                        contents.push(json!({"role": role, "parts": [{"text": text}]}));
                    }
                    let body = json!({ "contents": contents });
                    let res = self.client.post(&url).json(&body).send().await?;
                    let j: serde_json::Value = res.json().await?;
                    let text = j["candidates"][0]["content"]["parts"][0]["text"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("Google API error: {:?}", j))?;
                    return Ok(text.to_string());
                }
            };

            let body = serde_json::json!({
                "model": self.model,
                "messages": messages,
                "temperature": 0.8,
                "top_p": 0.9
            });

            let mut req = self.client.post(url)
                .header("Authorization", format!("Bearer {}", api_key))
                .json(&body);

            if matches!(self.provider, AiProvider::OpenRouter) {
                req = req.header("HTTP-Referer", "https://juda7w.app").header("X-Title", "Juda7w");
            }

            let res = req.send().await?;
            
            if res.status().is_success() {
                let j: serde_json::Value = res.json().await?;
                let text = j["choices"][0]["message"]["content"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("LLM API error: {:?}", j))?;
                return Ok(text.to_string());
            } else if res.status().as_u16() == 429 || res.status().is_server_error() {
                self.rotate_key();
                last_error = Some(anyhow::anyhow!("Status {}: {:?}", res.status(), res.text().await?));
                continue;
            } else {
                return Err(anyhow::anyhow!("API Error {}: {}", res.status(), res.text().await?));
            }
        }
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("All API keys failed")))
    }

    pub async fn ask(&mut self, question: &str, relevant_experiences: Vec<String>) -> Result<String> {
        let system_prompt = self.build_system_prompt(&relevant_experiences);
        
        let mut messages = vec![json!({"role": "system", "content": system_prompt})];
        
        // Keep only last 10 messages (5 exchanges)
        if self.conversation_history.len() > 10 {
            self.conversation_history = self.conversation_history[self.conversation_history.len() - 10..].to_vec();
        }
        
        messages.extend(self.conversation_history.clone());
        messages.push(json!({"role": "user", "content": question}));

        let response = self.call_ai_messages(messages).await?;
        
        self.conversation_history.push(json!({"role": "user", "content": question}));
        self.conversation_history.push(json!({"role": "assistant", "content": response.clone()}));

        Ok(response)
    }
}
