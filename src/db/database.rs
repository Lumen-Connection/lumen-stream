use std::fs;
use std::path::Path;

use chrono::Local;
use rusqlite::{params, Connection};

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct HistoryEntry {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub media_type: String,
    pub format: String,
    pub quality: String,
    pub file_path: String,
    pub folder_path: String,
    pub file_size: Option<i64>,
    pub created_at: String,
    pub favorite: bool,
    pub tags: String,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct FolderEntry {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub created_at: String,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Self {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("Failed to create database directory");
        }
        let conn = Connection::open(path).expect("Failed to open database");
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
                title TEXT NOT NULL,
                media_type TEXT NOT NULL,
                format TEXT NOT NULL,
                quality TEXT NOT NULL,
                file_path TEXT NOT NULL,
                folder_path TEXT NOT NULL,
                file_size INTEGER,
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS folders (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                path TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            ",
        )
        .expect("Failed to initialize database schema");
        // Migrações (ignoram erro se a coluna já existir).
        conn.execute(
            "ALTER TABLE history ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .ok();
        conn.execute(
            "ALTER TABLE history ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .ok();
        conn.execute("ALTER TABLE history ADD COLUMN tags TEXT NOT NULL DEFAULT ''", [])
            .ok();
        Database { conn }
    }

    pub fn add_history(
        &self,
        url: &str,
        title: &str,
        media_type: &str,
        format: &str,
        quality: &str,
        folder_path: &str,
        file_path: &str,
        file_size: Option<i64>,
    ) {
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        self.conn
            .execute(
                "INSERT INTO history (url, title, media_type, format, quality, file_path, folder_path, file_size, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![url, title, media_type, format, quality, file_path, folder_path, file_size, now],
            )
            .ok();
    }

    pub fn get_history(&self, media_type: &str, limit: usize) -> Vec<HistoryEntry> {
        self.query_history(media_type, limit, 0)
    }

    /// Itens na lixeira (deleted = 1).
    pub fn get_deleted_history(&self, media_type: &str, limit: usize) -> Vec<HistoryEntry> {
        self.query_history(media_type, limit, 1)
    }

    fn query_history(&self, media_type: &str, limit: usize, deleted: i64) -> Vec<HistoryEntry> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, url, title, media_type, format, quality, file_path, folder_path, file_size, created_at, favorite, tags
                 FROM history WHERE media_type = ?1 AND deleted = ?3 ORDER BY created_at DESC LIMIT ?2",
            )
            .unwrap();
        stmt.query_map(params![media_type, limit as i64, deleted], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                media_type: row.get(3)?,
                format: row.get(4)?,
                quality: row.get(5)?,
                file_path: row.get(6)?,
                folder_path: row.get(7)?,
                file_size: row.get(8)?,
                created_at: row.get(9)?,
                favorite: row.get::<_, i64>(10)? != 0,
                tags: row.get(11)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    /// Alterna o estado de favorito de um item.
    pub fn toggle_favorite(&self, id: i64) {
        self.conn
            .execute(
                "UPDATE history SET favorite = 1 - favorite WHERE id = ?1",
                params![id],
            )
            .ok();
    }

    /// Define as tags/categorias (texto livre separado por vírgula) de um item.
    pub fn set_tags(&self, id: i64, tags: &str) {
        self.conn
            .execute("UPDATE history SET tags = ?1 WHERE id = ?2", params![tags, id])
            .ok();
    }

    /// Move um item para a lixeira (soft delete).
    pub fn delete_history(&self, id: i64) {
        self.conn
            .execute("UPDATE history SET deleted = 1 WHERE id = ?1", params![id])
            .ok();
    }

    /// Restaura um item da lixeira.
    pub fn restore_history(&self, id: i64) {
        self.conn
            .execute("UPDATE history SET deleted = 0 WHERE id = ?1", params![id])
            .ok();
    }

    /// Move todos os itens de um tipo para a lixeira.
    pub fn clear_history(&self, media_type: &str) {
        self.conn
            .execute(
                "UPDATE history SET deleted = 1 WHERE media_type = ?1",
                params![media_type],
            )
            .ok();
    }

    /// Esvazia a lixeira (remove de vez) de um tipo.
    pub fn empty_trash(&self, media_type: &str) {
        self.conn
            .execute(
                "DELETE FROM history WHERE media_type = ?1 AND deleted = 1",
                params![media_type],
            )
            .ok();
    }

    /// Indica se uma URL já foi baixada (não excluída).
    pub fn url_exists(&self, url: &str) -> bool {
        if url.is_empty() {
            return false;
        }
        self.conn
            .query_row(
                "SELECT 1 FROM history WHERE url = ?1 AND deleted = 0 LIMIT 1",
                params![url],
                |_| Ok(()),
            )
            .is_ok()
    }

    /// Remove definitivamente itens da lixeira com mais de `days` dias.
    pub fn purge_old_trash(&self, days: i64) {
        self.conn
            .execute(
                "DELETE FROM history WHERE deleted = 1 AND created_at < datetime('now', 'localtime', ?1)",
                params![format!("-{} days", days)],
            )
            .ok();
    }

    /// Estatísticas (apenas itens não excluídos): (qtd, soma de tamanho) por tipo.
    pub fn stats(&self, media_type: &str) -> (i64, i64) {
        self.conn
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(file_size), 0) FROM history
                 WHERE media_type = ?1 AND deleted = 0",
                params![media_type],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap_or((0, 0))
    }

    /// Atualiza o caminho do arquivo de um item (ex.: após mover/arquivar).
    pub fn update_file_path(&self, id: i64, new_path: &str) {
        self.conn
            .execute(
                "UPDATE history SET file_path = ?1 WHERE id = ?2",
                params![new_path, id],
            )
            .ok();
    }

    /// Todos os itens ativos (não excluídos), de todos os tipos.
    pub fn all_active_history(&self) -> Vec<HistoryEntry> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, url, title, media_type, format, quality, file_path, folder_path, file_size, created_at, favorite, tags
                 FROM history WHERE deleted = 0 ORDER BY created_at DESC",
            )
            .unwrap();
        stmt.query_map([], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                media_type: row.get(3)?,
                format: row.get(4)?,
                quality: row.get(5)?,
                file_path: row.get(6)?,
                folder_path: row.get(7)?,
                file_size: row.get(8)?,
                created_at: row.get(9)?,
                favorite: row.get::<_, i64>(10)? != 0,
                tags: row.get(11)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    pub fn add_folder(&self, name: &str, path: &str) {
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        self.conn
            .execute(
                "INSERT OR IGNORE INTO folders (name, path, created_at) VALUES (?1, ?2, ?3)",
                params![name, path, now],
            )
            .ok();
    }

    pub fn delete_folder(&self, id: i64) {
        self.conn
            .execute("DELETE FROM folders WHERE id = ?1", params![id])
            .ok();
    }

    pub fn rename_folder(&self, id: i64, name: &str) {
        self.conn
            .execute(
                "UPDATE folders SET name = ?1 WHERE id = ?2",
                params![name, id],
            )
            .ok();
    }

    pub fn get_folders(&self) -> Vec<FolderEntry> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, name, path, created_at FROM folders ORDER BY created_at DESC",
            )
            .unwrap();
        stmt.query_map([], |row| {
            Ok(FolderEntry {
                id: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    #[allow(dead_code)]
    pub fn get_setting(&self, key: &str) -> Option<String> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM settings WHERE key = ?1")
            .ok()?;
        stmt.query_row(params![key], |row| row.get(0)).ok()
    }

    #[allow(dead_code)]
    pub fn set_setting(&self, key: &str, value: &str) {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
                params![key, value],
            )
            .ok();
    }
}
