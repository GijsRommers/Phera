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
