use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::Local;
use rusqlite::{params, Connection};

// Época global de escrita. Toda instância de `Database` — inclusive as abertas em
// threads de background para gravar downloads concluídos — incrementa estes
// contadores ao mutar os dados. A UI compara a época contra a do seu cache para
// saber quando recarregar, em vez de consultar o SQLite a cada frame. Como o
// processo é único, o `static` é compartilhado por todos os escritores, então a
// invalidação nunca é esquecida: ela mora no caminho de escrita, não nas chamadas.
//
// INVARIANTE: o incremento vem SEMPRE DEPOIS de a escrita ter sido executada.
// Incrementar antes abre uma janela em que o leitor (a UI repinta durante o
// download) observa a época nova, recarrega o cache sem a linha que ainda não foi
// gravada e memoriza a época nova — e então nunca mais recarrega, porque ninguém
// incrementa de novo. O sintoma é o histórico só aparecer no download seguinte.
static HISTORY_EPOCH: AtomicU64 = AtomicU64::new(0);
static FOLDERS_EPOCH: AtomicU64 = AtomicU64::new(0);

/// Época atual das escritas de histórico (muda a cada mutação de histórico).
pub fn history_epoch() -> u64 {
    HISTORY_EPOCH.load(Ordering::Relaxed)
}

/// Época atual das escritas de pastas (muda a cada mutação de pastas rastreadas).
pub fn folders_epoch() -> u64 {
    FOLDERS_EPOCH.load(Ordering::Relaxed)
}

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
        HISTORY_EPOCH.fetch_add(1, Ordering::Relaxed);
    }

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

    pub fn toggle_favorite(&self, id: i64) {
        self.conn
            .execute(
                "UPDATE history SET favorite = 1 - favorite WHERE id = ?1",
                params![id],
            )
            .ok();
        HISTORY_EPOCH.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_tags(&self, id: i64, tags: &str) {
        self.conn
            .execute("UPDATE history SET tags = ?1 WHERE id = ?2", params![tags, id])
            .ok();
        HISTORY_EPOCH.fetch_add(1, Ordering::Relaxed);
    }

    pub fn delete_history(&self, id: i64) {
        self.conn
            .execute("UPDATE history SET deleted = 1 WHERE id = ?1", params![id])
            .ok();
        HISTORY_EPOCH.fetch_add(1, Ordering::Relaxed);
    }

    pub fn restore_history(&self, id: i64) {
        self.conn
            .execute("UPDATE history SET deleted = 0 WHERE id = ?1", params![id])
            .ok();
        HISTORY_EPOCH.fetch_add(1, Ordering::Relaxed);
    }

    pub fn clear_history(&self, media_type: &str) {
        self.conn
            .execute(
                "UPDATE history SET deleted = 1 WHERE media_type = ?1",
                params![media_type],
            )
            .ok();
        HISTORY_EPOCH.fetch_add(1, Ordering::Relaxed);
    }

    pub fn empty_trash(&self, media_type: &str) {
        self.conn
            .execute(
                "DELETE FROM history WHERE media_type = ?1 AND deleted = 1",
                params![media_type],
            )
            .ok();
        HISTORY_EPOCH.fetch_add(1, Ordering::Relaxed);
    }

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

    pub fn purge_old_trash(&self, days: i64) {
        self.conn
            .execute(
                "DELETE FROM history WHERE deleted = 1 AND created_at < datetime('now', 'localtime', ?1)",
                params![format!("-{} days", days)],
            )
            .ok();
        HISTORY_EPOCH.fetch_add(1, Ordering::Relaxed);
    }

    /// Reescreve o prefixo de caminho em todo o histórico (usado ao renomear a
    /// pasta de downloads no rebrand, p/ os arquivos continuarem localizáveis).
    pub fn rewrite_path_prefix(&self, old: &str, new: &str) {
        self.conn
            .execute(
                "UPDATE history SET file_path = replace(file_path, ?1, ?2), \
                 folder_path = replace(folder_path, ?1, ?2)",
                params![old, new],
            )
            .ok();
        HISTORY_EPOCH.fetch_add(1, Ordering::Relaxed);
    }

    pub fn update_file_path(&self, id: i64, new_path: &str) {
        self.conn
            .execute(
                "UPDATE history SET file_path = ?1 WHERE id = ?2",
                params![new_path, id],
            )
            .ok();
        HISTORY_EPOCH.fetch_add(1, Ordering::Relaxed);
    }

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
        FOLDERS_EPOCH.fetch_add(1, Ordering::Relaxed);
    }

    pub fn delete_folder(&self, id: i64) {
        self.conn
            .execute("DELETE FROM folders WHERE id = ?1", params![id])
            .ok();
        FOLDERS_EPOCH.fetch_add(1, Ordering::Relaxed);
    }

    pub fn rename_folder(&self, id: i64, name: &str) {
        self.conn
            .execute(
                "UPDATE folders SET name = ?1 WHERE id = ?2",
                params![name, id],
            )
            .ok();
        FOLDERS_EPOCH.fetch_add(1, Ordering::Relaxed);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // Cada teste usa um arquivo próprio (dados isolados). A época é um `static`
    // global compartilhado pelo processo, então as asserções sobre ela são
    // sempre monotônicas (`>` / `!=`) para não dependerem da ordem dos testes.
    fn temp_db_path(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut p = std::env::temp_dir();
        p.push(format!("lumen_stream_test_{tag}_{nanos}.sqlite"));
        p
    }

    #[test]
    fn history_write_bumps_epoch_and_is_queryable() {
        let path = temp_db_path("hist");
        let db = Database::open(&path);
        let before = history_epoch();
        db.add_history("u", "Song", "music", "mp3", "best", "C:/x", "C:/x/s.mp3", Some(123));
        assert!(history_epoch() > before, "add_history deve incrementar a época de histórico");
        let all = db.all_active_history();
        assert!(
            all.iter().any(|h| h.title == "Song" && h.media_type == "music"),
            "o item gravado deve aparecer no histórico ativo"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn soft_delete_excludes_from_active_and_bumps_epoch() {
        let path = temp_db_path("del");
        let db = Database::open(&path);
        db.add_history("u", "Temp", "video", "mp4", "best", "C:/x", "C:/x/t.mp4", None);
        let id = db
            .all_active_history()
            .iter()
            .find(|h| h.title == "Temp")
            .expect("item recém-gravado deve existir")
            .id;
        let before = history_epoch();
        db.delete_history(id);
        assert!(history_epoch() > before, "delete_history deve incrementar a época");
        assert!(
            !db.all_active_history().iter().any(|h| h.id == id),
            "item deletado sai do conjunto ativo"
        );
        assert!(
            db.get_deleted_history("video", 100).iter().any(|h| h.id == id),
            "item deletado vai para a lixeira (recuperável)"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn folder_write_bumps_folders_epoch() {
        let path = temp_db_path("fold");
        let db = Database::open(&path);
        let before = folders_epoch();
        db.add_folder("Downloads", "C:/Downloads");
        assert!(folders_epoch() > before, "add_folder deve incrementar a época de pastas");
        assert!(db.get_folders().iter().any(|f| f.path == "C:/Downloads"));
        let before_del = folders_epoch();
        let id = db.get_folders().iter().find(|f| f.path == "C:/Downloads").unwrap().id;
        db.delete_folder(id);
        assert!(folders_epoch() > before_del, "delete_folder deve incrementar a época");
        assert!(!db.get_folders().iter().any(|f| f.path == "C:/Downloads"));
        let _ = std::fs::remove_file(&path);
    }

    // Propriedade crítica para o cache da UI: uma escrita feita por OUTRA instância
    // de `Database` (uma thread de background gravando um download concluído) muda a
    // época global compartilhada, então a UI sabe que precisa recarregar — e ao
    // recarregar, enxerga o novo item. Sem isso, downloads concluídos só apareceriam
    // após outra mutação (regressão que o mecanismo de época previne).
    #[test]
    fn background_writer_invalidates_via_shared_epoch() {
        let path = temp_db_path("shared");
        let ui_db = Database::open(&path);
        let cached_epoch = history_epoch(); // a "UI" memoriza a época ao carregar
        let _snapshot = ui_db.all_active_history();

        // Segunda instância no MESMO arquivo, como uma thread de background faria.
        let bg_db = Database::open(&path);
        bg_db.add_history("u", "FromBackground", "music", "mp3", "best", "C:/x", "C:/x/b.mp3", None);

        assert!(
            history_epoch() != cached_epoch,
            "a escrita da instância de background deve mudar a época global"
        );
        assert!(
            ui_db
                .all_active_history()
                .iter()
                .any(|h| h.title == "FromBackground"),
            "ao recarregar, a instância da UI enxerga a escrita da instância de background"
        );
        let _ = std::fs::remove_file(&path);
    }

    fn seeded_db(tag: &str) -> (PathBuf, Database, i64) {
        let path = temp_db_path(tag);
        let db = Database::open(&path);
        db.add_history("http://u/1", "Item", "music", "mp3", "best", "C:/x", "C:/x/i.mp3", None);
        let id = db.all_active_history()[0].id;
        (path, db, id)
    }

    #[test]
    fn favorite_toggles_on_and_off() {
        let (path, db, id) = seeded_db("fav");
        db.toggle_favorite(id);
        assert!(db.all_active_history()[0].favorite);
        db.toggle_favorite(id);
        assert!(!db.all_active_history()[0].favorite);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tags_are_saved_per_item() {
        let (path, db, id) = seeded_db("tags");
        db.set_tags(id, "rock, anos 80");
        assert_eq!(db.all_active_history()[0].tags, "rock, anos 80");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn restore_brings_item_back_from_trash() {
        let (path, db, id) = seeded_db("restore");
        db.delete_history(id);
        assert!(db.all_active_history().is_empty());
        db.restore_history(id);
        assert!(db.all_active_history().iter().any(|h| h.id == id));
        assert!(db.get_deleted_history("music", 10).is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn empty_trash_deletes_permanently() {
        let (path, db, id) = seeded_db("trash");
        db.delete_history(id);
        db.empty_trash("music");
        assert!(db.get_deleted_history("music", 10).is_empty());
        db.restore_history(id); // não há mais o que restaurar
        assert!(db.all_active_history().is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn url_exists_only_for_active_entries() {
        let (path, db, id) = seeded_db("urlexists");
        assert!(db.url_exists("http://u/1"));
        assert!(!db.url_exists("http://outro"));
        assert!(!db.url_exists(""), "url vazia nunca conta como duplicada");
        db.delete_history(id);
        assert!(!db.url_exists("http://u/1"), "item na lixeira não bloqueia novo download");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn clear_history_moves_only_that_media_type() {
        let path = temp_db_path("clear");
        let db = Database::open(&path);
        db.add_history("u1", "M", "music", "mp3", "best", "C:/x", "C:/x/m.mp3", None);
        db.add_history("u2", "V", "video", "mp4", "best", "C:/x", "C:/x/v.mp4", None);
        db.clear_history("music");
        let rest = db.all_active_history();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].media_type, "video");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn update_and_rewrite_paths() {
        let (path, db, id) = seeded_db("paths");
        db.update_file_path(id, "D:/novo/i.mp3");
        assert_eq!(db.all_active_history()[0].file_path, "D:/novo/i.mp3");
        db.rewrite_path_prefix("D:/novo", "D:/LumenStream");
        let h = &db.all_active_history()[0];
        assert_eq!(h.file_path, "D:/LumenStream/i.mp3");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn folders_rename_and_dedupe() {
        let path = temp_db_path("foldops");
        let db = Database::open(&path);
        db.add_folder("Docs", "C:/Docs");
        db.add_folder("Duplicada", "C:/Docs"); // caminho é UNIQUE: ignorada
        assert_eq!(db.get_folders().len(), 1);
        let id = db.get_folders()[0].id;
        db.rename_folder(id, "Documentos");
        assert_eq!(db.get_folders()[0].name, "Documentos");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn settings_roundtrip_and_overwrite() {
        let path = temp_db_path("settings");
        let db = Database::open(&path);
        assert_eq!(db.get_setting("chave"), None);
        db.set_setting("chave", "v1");
        assert_eq!(db.get_setting("chave"), Some("v1".to_string()));
        db.set_setting("chave", "v2");
        assert_eq!(db.get_setting("chave"), Some("v2".to_string()));
        let _ = std::fs::remove_file(&path);
    }
}
