use rusqlite::{ffi::sqlite3_auto_extension, Connection};
use sqlite_vec::sqlite3_vec_init;
use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};
use anyhow::Result;

pub struct Database {
    conn: Connection,
    model: TextEmbedding,
}

impl Database {
    pub fn new() -> Result<Self> {
        // Register the sqlite-vec extension globally.
        // This must be called before opening any database connections.
        unsafe {
            sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
        }

        let conn = Connection::open("experiences.db")?;
        
        // Initialize the vector table
        // BGE-Small-EN-V1.5 has 384 dimensions
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

    pub fn add_experience(&mut self, text: &str) -> Result<()> {
        // FastEmbed 5.x might require mutable access or we might need to handle Arc better
        // If Arc<TextEmbedding> is used, we might need a Mutex if embed takes &mut self
        // Let's try to get a mutable reference if possible or change the type.
        
        // Actually, let's check if we can just use &self. 
        // If the compiler says it needs &mut self, we'll wrap it in a Mutex.
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

        // Note: sqlite-vec uses 'k' for the number of results in some versions, 
        // or just LIMIT. The MATCH syntax for vec0 is: 
        // WHERE embedding MATCH ? AND k = ?
        
        let rows = stmt.query_map(rusqlite::params![query_blob, limit], |row| {
            row.get(0)
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        
        Ok(results)
    }
}

fn vec_to_blob(v: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(v.len() * 4);
    for &f in v {
        bytes.extend_from_slice(&f.to_le_bytes());
    }
    bytes
}
