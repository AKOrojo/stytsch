#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::db::Database;
    use crate::schema::{History, SearchFilter};

    fn temp_db() -> (Database, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open_at(&dir.path().join("test.db")).unwrap();
        (db, dir)
    }

    fn make_entry(cmd: &str, cwd: &str, exit: i32, ts_offset: i64) -> History {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i64;
        History {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: now + ts_offset,
            duration: 100_000_000,
            exit,
            command: cmd.to_string(),
            cwd: cwd.to_string(),
            session: "test-session".to_string(),
            hostname: "test-host".to_string(),
            deleted_at: None,
        }
    }

    // ── Database basics ─────────────────────────────────────────────

    #[test]
    fn test_db_insert_and_count() {
        let (db, _d) = temp_db();
        assert_eq!(db.count().unwrap(), 0);

        db.insert(&make_entry("dir", r"C:\", 0, 0)).unwrap();
        assert_eq!(db.count().unwrap(), 1);

        db.insert(&make_entry("echo hi", r"C:\", 0, 1)).unwrap();
        assert_eq!(db.count().unwrap(), 2);
    }

    #[test]
    fn test_db_dedup_by_timestamp_cwd_command() {
        let (db, _d) = temp_db();
        let entry = make_entry("echo hi", r"C:\", 0, 0);
        db.insert(&entry).unwrap();
        db.insert(&entry).unwrap(); // same timestamp+cwd+command
        assert_eq!(db.count().unwrap(), 1);
    }

    #[test]
    fn test_db_different_timestamps_not_deduped() {
        let (db, _d) = temp_db();
        db.insert(&make_entry("echo hi", r"C:\", 0, 0)).unwrap();
        db.insert(&make_entry("echo hi", r"C:\", 0, 1_000_000)).unwrap();
        assert_eq!(db.count().unwrap(), 2);
    }

    // ── Search and filtering ────────────────────────────────────────

    #[test]
    fn test_search_all() {
        let (db, _d) = temp_db();
        for i in 0..10 {
            db.insert(&make_entry(&format!("cmd{i}"), r"C:\", 0, i * 1_000_000)).unwrap();
        }
        let results = db.search(&SearchFilter::new(100)).unwrap();
        assert_eq!(results.len(), 10);
        // Most recent first.
        assert!(results[0].command == "cmd9");
    }

    #[test]
    fn test_search_by_query() {
        let (db, _d) = temp_db();
        db.insert(&make_entry("cargo build", r"C:\proj", 0, 0)).unwrap();
        db.insert(&make_entry("cargo test", r"C:\proj", 0, 1_000_000)).unwrap();
        db.insert(&make_entry("git status", r"C:\proj", 0, 2_000_000)).unwrap();

        let results = db.search(&SearchFilter {
            query: Some("cargo".to_string()),
            limit: 100,
            ..Default::default()
        }).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_by_cwd() {
        let (db, _d) = temp_db();
        db.insert(&make_entry("npm start", r"C:\frontend", 0, 0)).unwrap();
        db.insert(&make_entry("cargo run", r"C:\backend", 0, 1_000_000)).unwrap();

        let results = db.search(&SearchFilter {
            cwd: Some(r"C:\frontend".to_string()),
            limit: 100,
            ..Default::default()
        }).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].command, "npm start");
    }

    #[test]
    fn test_search_by_exit_code() {
        let (db, _d) = temp_db();
        db.insert(&make_entry("good cmd", r"C:\", 0, 0)).unwrap();
        db.insert(&make_entry("bad cmd", r"C:\", 1, 1_000_000)).unwrap();
        db.insert(&make_entry("worse cmd", r"C:\", 127, 2_000_000)).unwrap();

        let results = db.search(&SearchFilter {
            exit: Some(0),
            limit: 100,
            ..Default::default()
        }).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].command, "good cmd");
    }

    #[test]
    fn test_search_by_hostname() {
        let (db, _d) = temp_db();
        let mut e1 = make_entry("local cmd", r"C:\", 0, 0);
        e1.hostname = "laptop".to_string();
        let mut e2 = make_entry("remote cmd", r"C:\", 0, 1_000_000);
        e2.hostname = "server".to_string();

        db.insert(&e1).unwrap();
        db.insert(&e2).unwrap();

        let results = db.search(&SearchFilter {
            hostname: Some("server".to_string()),
            limit: 100,
            ..Default::default()
        }).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].command, "remote cmd");
    }

    #[test]
    fn test_search_limit_and_offset() {
        let (db, _d) = temp_db();
        for i in 0..20 {
            db.insert(&make_entry(&format!("cmd{i}"), r"C:\", 0, i * 1_000_000)).unwrap();
        }

        let results = db.search(&SearchFilter { limit: 5, ..Default::default() }).unwrap();
        assert_eq!(results.len(), 5);

        let results = db.search(&SearchFilter { limit: 5, offset: 15, ..Default::default() }).unwrap();
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_search_combined_filters() {
        let (db, _d) = temp_db();
        db.insert(&make_entry("cargo build", r"C:\proj", 0, 0)).unwrap();
        db.insert(&make_entry("cargo build", r"C:\proj", 1, 1_000_000)).unwrap();
        db.insert(&make_entry("cargo test", r"C:\proj", 0, 2_000_000)).unwrap();
        db.insert(&make_entry("cargo build", r"C:\other", 0, 3_000_000)).unwrap();

        let results = db.search(&SearchFilter {
            query: Some("build".to_string()),
            cwd: Some(r"C:\proj".to_string()),
            exit: Some(0),
            limit: 100,
            ..Default::default()
        }).unwrap();
        assert_eq!(results.len(), 1);
    }

    // ── Soft delete ─────────────────────────────────────────────────

    #[test]
    fn test_soft_delete() {
        let (db, _d) = temp_db();
        let entry = make_entry("secret", r"C:\", 0, 0);
        let id = entry.id.clone();
        db.insert(&entry).unwrap();
        assert_eq!(db.count().unwrap(), 1);

        db.soft_delete(&id, 999).unwrap();
        assert_eq!(db.count().unwrap(), 0);
        // Excluded from search.
        assert_eq!(db.search(&SearchFilter::new(100)).unwrap().len(), 0);
    }

    // ── Pruning ─────────────────────────────────────────────────────

    #[test]
    fn test_prune_oldest() {
        let (db, _d) = temp_db();
        for i in 0..50 {
            db.insert(&make_entry(&format!("cmd{i}"), r"C:\", 0, i * 1_000_000)).unwrap();
        }
        assert_eq!(db.count().unwrap(), 50);

        let deleted = db.prune_oldest(30).unwrap();
        assert_eq!(deleted, 30);
        assert_eq!(db.count().unwrap(), 20);

        // Should have kept the most recent 20.
        let results = db.search(&SearchFilter::new(100)).unwrap();
        assert_eq!(results[0].command, "cmd49");
        assert_eq!(results[19].command, "cmd30");
    }

    #[test]
    fn test_prune_more_than_exists() {
        let (db, _d) = temp_db();
        db.insert(&make_entry("only one", r"C:\", 0, 0)).unwrap();
        let deleted = db.prune_oldest(100).unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(db.count().unwrap(), 0);
    }

    // ── Concurrent access (WAL mode) ───────────────────────────────

    #[test]
    fn test_concurrent_writes() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("concurrent.db");

        // Simulate 5 "sessions" writing concurrently.
        let handles: Vec<_> = (0..5).map(|session| {
            let path = db_path.clone();
            std::thread::spawn(move || {
                let db = Database::open_at(&path).unwrap();
                for i in 0..20 {
                    let mut entry = make_entry(
                        &format!("s{session}_cmd{i}"),
                        r"C:\",
                        0,
                        (session * 1000 + i) * 1_000_000,
                    );
                    entry.session = format!("session-{session}");
                    db.insert(&entry).unwrap();
                }
            })
        }).collect();

        for h in handles {
            h.join().unwrap();
        }

        let db = Database::open_at(&db_path).unwrap();
        assert_eq!(db.count().unwrap(), 100); // 5 sessions * 20 commands
    }

    #[test]
    fn test_concurrent_read_while_writing() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("rw.db");

        // Pre-populate.
        let db = Database::open_at(&db_path).unwrap();
        for i in 0..50 {
            db.insert(&make_entry(&format!("init{i}"), r"C:\", 0, i * 1_000_000)).unwrap();
        }

        // Writer thread.
        let wp = db_path.clone();
        let writer = std::thread::spawn(move || {
            let db = Database::open_at(&wp).unwrap();
            for i in 50..100 {
                db.insert(&make_entry(&format!("new{i}"), r"C:\", 0, i * 1_000_000)).unwrap();
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        });

        // Reader thread — should never block.
        let rp = db_path.clone();
        let reader = std::thread::spawn(move || {
            let db = Database::open_at(&rp).unwrap();
            for _ in 0..20 {
                let count = db.count().unwrap();
                assert!(count >= 50); // Should always see at least initial data.
                let _ = db.search(&SearchFilter::new(10)).unwrap();
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        });

        writer.join().unwrap();
        reader.join().unwrap();

        let db = Database::open_at(&db_path).unwrap();
        assert_eq!(db.count().unwrap(), 100);
    }

    // ── Config ──────────────────────────────────────────────────────

    #[test]
    fn test_config_defaults() {
        let config = Config::default();
        assert_eq!(config.search_mode, "fzf");
        assert_eq!(config.max_history, 100_000);
        assert!(config.sync_server.is_none());
    }

    // ── Lua script syntax check ─────────────────────────────────────

    #[test]
    fn test_lua_script_embedded() {
        let script = include_str!("../scripts/stytsch.lua");
        assert!(script.contains("stytsch_search"));
        assert!(script.contains("rl.setbinding"));
        assert!(script.contains("clink.onendedit"));
        assert!(script.contains("clink.onbeginedit"));
        assert!(script.contains("stytsch_toggle"));
    }
}
