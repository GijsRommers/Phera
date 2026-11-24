//! `unset($result)` releases SQLite locks before the next operation on the database.

use phera_sqlite::{Cell, Database};

#[test]
fn cursor_drop_releases_locks_before_next_command() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let database = Database::open(":memory:".into(), 6, 0).await.unwrap();
            database
                .exec(
                    "ATTACH ':memory:' AS extra; CREATE TABLE extra.a(x); \
                       INSERT INTO extra.a VALUES(42)"
                        .into(),
                )
                .await
                .unwrap();
            for _ in 0..100 {
                let cursor = database
                    .prepare("SELECT x FROM extra.a".into())
                    .await
                    .unwrap();
                assert!(matches!(
                    cursor.execute(Vec::new()).await.unwrap().first.unwrap()[0],
                    Cell::Integer(42)
                ));
                drop(cursor);
                database
                    .exec(
                        "DROP TABLE extra.a; CREATE TABLE extra.a(x); \
                               INSERT INTO extra.a VALUES(42)"
                            .into(),
                    )
                    .await
                    .unwrap();
            }
            database.exec("DETACH extra".into()).await.unwrap();
            database.close().await.unwrap();
        });
}

#[test]
fn statements_retain_database_and_explicit_close_invalidates_them() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let database = Database::open(":memory:".into(), 6, 0).await.unwrap();
            let cursor = database.prepare("SELECT 42".into()).await.unwrap();
            let other = Database::open(":memory:".into(), 6, 0).await.unwrap();
            other.exec("CREATE TABLE isolated(x)".into()).await.unwrap();
            assert!(
                database
                    .prepare("SELECT * FROM isolated".into())
                    .await
                    .is_err()
            );
            drop(database);
            assert!(cursor.execute(Vec::new()).await.unwrap().first.is_some());
            let closed = other.prepare("SELECT 1".into()).await.unwrap();
            other.close().await.unwrap();
            assert!(closed.execute(Vec::new()).await.is_err());
        });
}

#[test]
fn pooled_workers_isolate_sequential_and_failed_connections() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            for round in 0..32 {
                // Invalid flags must fail cleanly without poisoning a pooled worker.
                assert!(Database::open(":memory:".into(), -1, 0).await.is_err());
                let db = Database::open(":memory:".into(), 6, 0).await.unwrap();
                // Previous round's :memory: table must not survive on a reused worker.
                assert!(db.prepare("SELECT * FROM temp_only".into()).await.is_err());
                db.exec(format!(
                    "CREATE TABLE temp_only(v); INSERT INTO temp_only VALUES({round})"
                ))
                .await
                .unwrap();
                let stmt = db.prepare("SELECT v FROM temp_only".into()).await.unwrap();
                assert!(matches!(
                    stmt.execute(Vec::new()).await.unwrap().first.unwrap()[0],
                    Cell::Integer(v) if v == round
                ));
                drop(stmt);
                db.close().await.unwrap();
            }
        });
}

#[test]
fn clean_readonly_file_connection_sees_writes_and_invalidates_on_replace_or_mutation() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("cache_test.sqlite");
            let path_str = path.to_string_lossy().into_owned();
            // Seed WAL database.
            let seed = Database::open(path_str.clone(), 6, 5000).await.unwrap();
            seed.exec(
                "PRAGMA journal_mode=WAL; CREATE TABLE items(v INT); INSERT INTO items VALUES(10)"
                    .into(),
            )
            .await
            .unwrap();
            drop(seed);

            // Clean read-only sessions (candidate for worker connection cache).
            for _ in 0..8 {
                let db = Database::open(path_str.clone(), 6, 5000).await.unwrap();
                let stmt = db.prepare("SELECT v FROM items".into()).await.unwrap();
                let exec = stmt.execute(Vec::new()).await.unwrap();
                assert!(matches!(exec.first.unwrap()[0], Cell::Integer(10)));
                drop(stmt);
                drop(db);
            }

            // Concurrent/subsequent writer updates WAL; cached readers must see 20 immediately.
            let writer = Database::open(path_str.clone(), 6, 5000).await.unwrap();
            writer.exec("UPDATE items SET v = 20".into()).await.unwrap();
            drop(writer);

            for _ in 0..8 {
                let db = Database::open(path_str.clone(), 6, 5000).await.unwrap();
                assert_eq!(db.changes().await.unwrap(), 0);
                assert_eq!(db.last_id().await.unwrap(), 0);
                assert!(!db.transaction().await.unwrap());
                let stmt = db.prepare("SELECT v FROM items".into()).await.unwrap();
                let exec = stmt.execute(Vec::new()).await.unwrap();
                assert!(matches!(exec.first.unwrap()[0], Cell::Integer(20)));
                drop(stmt);
                drop(db);
            }

            // Replacing the file on disk (new inode) must invalidate any cached connection.
            let replacement = dir.path().join("replacement.sqlite");
            let rep_str = replacement.to_string_lossy().into_owned();
            let rep = Database::open(rep_str.clone(), 6, 5000).await.unwrap();
            rep.exec("CREATE TABLE items(v INT); INSERT INTO items VALUES(99)".into())
                .await
                .unwrap();
            rep.close().await.unwrap();
            std::fs::rename(&replacement, &path).unwrap();

            let after = Database::open(path_str, 6, 5000).await.unwrap();
            let stmt = after.prepare("SELECT v FROM items".into()).await.unwrap();
            let exec = stmt.execute(Vec::new()).await.unwrap();
            assert!(matches!(exec.first.unwrap()[0], Cell::Integer(99)));
            drop(stmt);
            after.close().await.unwrap();
        });
}
