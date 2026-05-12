use rusqlite::{ffi::sqlite3_auto_extension, Connection};
use sqlite_vec::sqlite3_vec_init;
use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};
use anyhow::Result;
use serde::Serialize;

#[derive(Serialize, Debug, Clone)]
pub struct PredefinedItem {
    pub id: Option<i64>,
    pub title: String,
    pub content: String,
}

pub struct Database {
    conn: Connection,
    model: TextEmbedding,
}

impl Database {
    pub fn new() -> Result<Self> {
        // Register the sqlite-vec extension globally.
        unsafe {
            sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
        }

        let conn = Connection::open("experiences.db")?;
        
        // Initialize the vector table
        conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS vec_experiences USING vec0(
                embedding float[384]
            );",
            [],
        )?;
        
        // Metadata table to store the actual text
        conn.execute(
            "CREATE TABLE IF NOT EXISTS experiences_meta (
                rowid INTEGER PRIMARY KEY,
                content TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );",
            [],
        )?;

        // Responses table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS responses (
                id INTEGER PRIMARY KEY,
                title TEXT UNIQUE NOT NULL,
                content TEXT NOT NULL
            );",
            [],
        )?;

        // Lifesavers table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS lifesavers (
                id INTEGER PRIMARY KEY,
                title TEXT UNIQUE NOT NULL,
                content TEXT NOT NULL
            );",
            [],
        )?;

        println!("Loading embedding model...");
        let mut options = InitOptions::default();
        options.model_name = EmbeddingModel::BGESmallENV15;
        options.show_download_progress = true;
        
        let model = TextEmbedding::try_new(options)?;

        Ok(Self { 
            conn, 
            model 
        })
    }

    pub fn get_responses(&self) -> Result<Vec<PredefinedItem>> {
        let mut stmt = self.conn.prepare("SELECT id, title, content FROM responses")?;
        let rows = stmt.query_map([], |row| {
            Ok(PredefinedItem {
                id: Some(row.get(0)?),
                title: row.get(1)?,
                content: row.get(2)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn get_lifesavers(&self) -> Result<Vec<PredefinedItem>> {
        let mut stmt = self.conn.prepare("SELECT id, title, content FROM lifesavers")?;
        let rows = stmt.query_map([], |row| {
            Ok(PredefinedItem {
                id: Some(row.get(0)?),
                title: row.get(1)?,
                content: row.get(2)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn save_response(&self, title: &str, content: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO responses (title, content) VALUES (?, ?) 
             ON CONFLICT(title) DO UPDATE SET content = excluded.content",
            [title, content],
        )?;
        Ok(())
    }

    pub fn save_lifesaver(&self, title: &str, content: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO lifesavers (title, content) VALUES (?, ?) 
             ON CONFLICT(title) DO UPDATE SET content = excluded.content",
            [title, content],
        )?;
        Ok(())
    }

    pub fn add_experience(&mut self, text: &str) -> Result<()> {
        let embeddings = self.model.embed(vec![text.to_string()], None)?;
        let embedding = &embeddings[0];
        let blob = vec_to_blob(embedding);

        let tx = self.conn.transaction()?;
        
        tx.execute(
            "INSERT INTO experiences_meta (content) VALUES (?)",
            [text],
        )?;
        
        let rowid = tx.last_insert_rowid();
        
        tx.execute(
            "INSERT INTO vec_experiences(rowid, embedding) VALUES (?, ?)",
            rusqlite::params![rowid, blob],
        )?;
        
        tx.commit()?;
        Ok(())
    }

    pub fn query_experiences(&mut self, query: &str, limit: usize) -> Result<Vec<String>> {
        let embeddings = self.model.embed(vec![query.to_string()], None)?;
        let query_blob = vec_to_blob(&embeddings[0]);

        let mut stmt = self.conn.prepare(
            "SELECT m.content 
             FROM vec_experiences v
             JOIN experiences_meta m ON v.rowid = m.rowid
             WHERE v.embedding MATCH ?
               AND k = ?
             ORDER BY distance"
        )?;

        let rows = stmt.query_map(rusqlite::params![query_blob, limit], |row| {
            row.get(0)
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        
        Ok(results)
    }

    pub fn get_experience_count(&self) -> Result<i64> {
        let mut stmt = self.conn.prepare("SELECT COUNT(*) FROM experiences_meta")?;
        let count: i64 = stmt.query_row([], |row| row.get(0))?;
        Ok(count)
    }

    pub fn delete_response(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM responses WHERE id = ?", [id])?;
        Ok(())
    }

    pub fn delete_lifesaver(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM lifesavers WHERE id = ?", [id])?;
        Ok(())
    }
}

fn vec_to_blob(v: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(v.len() * 4);
    for &f in v {
        bytes.extend_from_slice(&f.to_le_bytes());
    }
    bytes
}

