// storage.rs — SQLite 本地持久化（KV 表存 JSON，替代前端 localStorage）
//
// 设计：一张 kv 表，key 为 "settings" / "watchlist" / "ai_cache"，value 是 JSON 串。
// 前端原样存取 JSON，迁移成本最低；以后要做结构化查询再拆表。
// 数据库文件放在 Tauri app_data_dir（Win: %APPDATA%/com.ruststock.app/，Mac: ~/Library/Application Support/...）。

use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Db(pub Mutex<Connection>);

pub fn init_db(dir: PathBuf) -> Result<Db, String> {
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建数据目录失败: {e}"))?;
    let path = dir.join("rust-stock.db");
    let conn = Connection::open(&path).map_err(|e| format!("打开数据库失败: {e}"))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS kv (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
         );",
    )
    .map_err(|e| format!("建表失败: {e}"))?;
    Ok(Db(Mutex::new(conn)))
}

pub fn kv_get(db: &Db, key: &str) -> Result<Option<String>, String> {
    let conn = db.0.lock().map_err(|_| "数据库锁中毒")?;
    let mut stmt = conn
        .prepare("SELECT value FROM kv WHERE key = ?1")
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query([key]).map_err(|e| e.to_string())?;
    match rows.next().map_err(|e| e.to_string())? {
        Some(row) => Ok(Some(row.get::<_, String>(0).map_err(|e| e.to_string())?)),
        None => Ok(None),
    }
}

pub fn kv_set(db: &Db, key: &str, value: &str) -> Result<(), String> {
    let conn = db.0.lock().map_err(|_| "数据库锁中毒")?;
    conn.execute(
        "INSERT INTO kv (key, value, updated_at) VALUES (?1, ?2, strftime('%s','now'))
         ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = strftime('%s','now')",
        [key, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_db() -> Db {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS kv (
                key TEXT PRIMARY KEY, value TEXT NOT NULL,
                updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now')));",
        )
        .unwrap();
        Db(Mutex::new(conn))
    }

    #[test]
    fn test_kv_roundtrip() {
        let db = mem_db();
        assert_eq!(kv_get(&db, "settings").unwrap(), None);
        kv_set(&db, "settings", r#"{"source":"sina","interval":10}"#).unwrap();
        assert_eq!(
            kv_get(&db, "settings").unwrap().unwrap(),
            r#"{"source":"sina","interval":10}"#
        );
        // 覆盖更新
        kv_set(&db, "settings", r#"{"source":"eastmoney"}"#).unwrap();
        assert_eq!(
            kv_get(&db, "settings").unwrap().unwrap(),
            r#"{"source":"eastmoney"}"#
        );
    }

    #[test]
    fn test_kv_unicode_json() {
        let db = mem_db();
        let v = r#"{"sh600519":{"analysis":"基本面稳健，给出 +72","score":72}}"#;
        kv_set(&db, "ai_cache", v).unwrap();
        assert_eq!(kv_get(&db, "ai_cache").unwrap().unwrap(), v);
    }
}
