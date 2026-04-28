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
    groq_key: String,
    openrouter_key: String,
    google_key: String,
    client: Client,
    provider: AiProvider,
    model: String,
    /// Chunks loaded from contex.md at startup
    context_chunks: Vec<String>,
    /// Master prompt template loaded from prompt.md
    prompt_template: String,
}

impl GroqBrain {
    pub fn new(groq_key: String, openrouter_key: String, google_key: String) -> Self {
        let context_chunks = Self::load_context_file("contex.md");
        let prompt_template = Self::load_prompt_file("prompt.md");

        if context_chunks.is_empty() {
            println!("⚠️  contex.md is empty or not found.");
        } else {
            println!("📄 Loaded {} context chunk(s) from contex.md", context_chunks.len());
        }

        if prompt_template.is_empty() {
            println!("⚠️  prompt.md is empty or not found. Using default prompt.");
        } else {
            println!("🎯 Loaded master prompt from prompt.md");
        }

        Self {
            groq_key,
            openrouter_key,
            google_key,
            client: Client::new(),
            provider: AiProvider::Google,
            model: "gemini-2.5-flash".to_string(),
            context_chunks,
            prompt_template,
        }
    }

    /// Load prompt.md as the master prompt template
    fn load_prompt_file(path: &str) -> String {
        fs::read_to_string(path).unwrap_or_default().trim().to_string()
    }

    /// Build the full system prompt by injecting context and question into the template
    fn build_prompt(&self, question: &str) -> String {
        let personal_context = if self.context_chunks.is_empty() {
            "No personal context provided.".to_string()
        } else {
            self.context_chunks.iter().take(3).cloned().collect::<Vec<_>>().join("\n\n---\n\n")
        };

        if self.prompt_template.is_empty() {
            // Fallback if prompt.md is missing
            return format!(
                "You are an elite interview coach. Always respond in English using a natural STAR narrative.\nContext:\n{}\nQuestion: {}",
                personal_context, question
            );
        }

        self.prompt_template
            .replace("{{CONTEXT}}", &personal_context)
            .replace("{{QUESTION}}", question)
    }

    /// Read contex.md and split into chunks if too large
    fn load_context_file(path: &str) -> Vec<String> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c.trim().to_string(),
            Err(_) => return vec![],
        };

        if content.is_empty() {
            return vec![];
        }

        if content.len() <= CHUNK_SIZE {
            return vec![content];
        }

        // Split by paragraphs first (double newline), then by size
        let mut chunks: Vec<String> = Vec::new();
        let mut current = String::new();

        for paragraph in content.split("\n\n") {
            if current.len() + paragraph.len() + 2 > CHUNK_SIZE && !current.is_empty() {
                chunks.push(current.trim().to_string());
                current = String::new();
            }
            if !current.is_empty() {
                current.push_str("\n\n");
            }
            current.push_str(paragraph);
        }

        if !current.trim().is_empty() {
            chunks.push(current.trim().to_string());
        }

        chunks
    }

    pub fn update_context(&mut self, text: &str) {
        let lower = text.to_lowercase();
        if lower.contains("google") || lower.contains("gemini") {
            self.provider = AiProvider::Google;
            self.model = "gemini-flash-lite-latest".to_string();
        } else if lower.contains("groq") || lower.contains("fast") {
            self.provider = AiProvider::Groq;
            self.model = "llama-3.1-8b-instant".to_string();
        }
    }

    pub fn get_model_name(&self) -> &str {
        match self.provider {
            AiProvider::Groq => "GROQ",
            AiProvider::OpenRouter => "OPENROUTER",
            AiProvider::Google => "GOOGLE",
        }
    }

    /// Generate 3 short answer options based on the question and personal context
    pub async fn generate_options(&self, question: &str) -> Result<Vec<String>> {
        let personal_context = if self.context_chunks.is_empty() {
            "No personal context provided.".to_string()
        } else {
            self.context_chunks.iter().take(3).cloned().collect::<Vec<_>>().join("\n\n---\n\n")
        };

        let prompt = format!(
            "Based on this professional profile:\n{}\n\nFor the interview question: \"{}\"\n\nGenerate exactly 3 short answer options (one line each) that the candidate could use to answer. Each option must reference a DIFFERENT specific situation, project, or skill from their profile. Be concrete and specific.\n\nRespond ONLY as a numbered list:\n1. ...\n2. ...\n3. ...",
            personal_context, question
        );

        let raw = self.call_ai_raw(&prompt).await?;
        
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

    /// Internal: call the AI with a raw prompt, returns text directly
    async fn call_ai_raw(&self, prompt: &str) -> Result<String> {
        match self.provider {
            AiProvider::Google => {
                let url = format!(
                    "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                    self.model, self.google_key
                );
                let body = serde_json::json!({
                    "contents": [{"role": "user", "parts": [{"text": prompt}]}]
                });
                let res = self.client.post(&url).json(&body).send().await?;
                let j: serde_json::Value = res.json().await?;
                
                // Log the full response for debugging
                println!("🔍 Google raw response: {}", serde_json::to_string_pretty(&j).unwrap_or_default());

                let text = j["candidates"][0]["content"]["parts"][0]["text"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Google API error: {:?}", j))?;
                Ok(text.to_string())
            },
            AiProvider::Groq | AiProvider::OpenRouter => {
                let (url, api_key) = match self.provider {
                    AiProvider::Groq => ("https://api.groq.com/openai/v1/chat/completions", &self.groq_key),
                    AiProvider::OpenRouter => ("https://openrouter.ai/api/v1/chat/completions", &self.openrouter_key),
                    _ => unreachable!(),
                };

                let body = serde_json::json!({
                    "model": self.model,
                    "messages": [{"role": "user", "content": prompt}]
                });

                let mut req = self.client.post(url)
                    .header("Authorization", format!("Bearer {}", api_key))
                    .json(&body);

                if matches!(self.provider, AiProvider::OpenRouter) {
                    req = req.header("HTTP-Referer", "https://juda7w.app").header("X-Title", "Juda7w");
                }

                let res = req.send().await?;
                let j: serde_json::Value = res.json().await?;
                
                println!("🔍 LLM raw response: {}", serde_json::to_string_pretty(&j).unwrap_or_default());

                let text = j["choices"][0]["message"]["content"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("LLM API error: {:?}", j))?;
                Ok(text.to_string())
            }
        }
    }

    pub async fn ask(&self, question: &str, _experiences: Vec<String>) -> Result<String> {
        // The full prompt is built from prompt.md + contex.md + question
        let full_prompt = self.build_prompt(question);

        let (url, api_key) = match self.provider {
            AiProvider::Groq => ("https://api.groq.com/openai/v1/chat/completions", &self.groq_key),
            AiProvider::OpenRouter => ("https://openrouter.ai/api/v1/chat/completions", &self.openrouter_key),
            AiProvider::Google => {
                let url = format!(
                    "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                    self.model, self.google_key
                );
                // For Google, the full_prompt already contains everything
                let body = json!({
                    "contents": [{"role": "user", "parts": [{"text": full_prompt}]}]
                });
                let res = self.client.post(&url).json(&body).send().await?;
                let j: serde_json::Value = res.json().await?;
                return Ok(j["candidates"][0]["content"]["parts"][0]["text"]
                    .as_str().unwrap_or("").to_string());
            }
        };

        let body = json!({
            "model": self.model,
            "messages": [
                {"role": "user", "content": full_prompt}
            ]
        });

        let mut req = self.client.post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body);

        if matches!(self.provider, AiProvider::OpenRouter) {
            req = req.header("HTTP-Referer", "https://juda7w.app")
                     .header("X-Title", "Juda7w AI Assistant");
        }

        let res = req.send().await?;
        let json_res: serde_json::Value = res.json().await?;
        let answer = json_res["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("AI Error: {:?}", json_res))?;

        Ok(answer.to_string())
    }

    async fn ask_google(&self, url: &str, system: &str, user: &str) -> Result<String> {
        let body = json!({
            "contents": [
                {
                    "role": "user",
                    "parts": [{"text": format!("{}\n\nQuestion: {}", system, user)}]
                }
            ]
        });

        let res = self.client.post(url).json(&body).send().await?;
        let json_res: serde_json::Value = res.json().await?;
        let answer = json_res["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Google AI Error: {:?}", json_res))?;

        Ok(answer.to_string())
    }
}
