//! Reset must remove each previous borrower's server-side state.

use phera_extension_api::Value;

use super::{rows, scalar};
use crate::drivers::Prepared;
use crate::placeholders::Sql;
use crate::pool::Lease;

pub async fn dirty(driver: &mut Lease, name: &str) {
    driver
        .run_unprepared("CREATE TEMPORARY TABLE phera_pool_dirty (id INT)")
        .await
        .unwrap();
    let (variable, prepare) = if name == "pgsql" {
        (
            "SET application_name = 'dirty'",
            "PREPARE dirty_statement AS SELECT 42",
        )
    } else {
        (
            "SET @phera_pool_dirty = 42",
            "PREPARE dirty_statement FROM 'SELECT 42'",
        )
    };
    driver.run_unprepared(variable).await.unwrap();
    driver.run_unprepared(prepare).await.unwrap();
    if name == "pgsql" {
        driver
            .run_unprepared("CREATE TEMP SEQUENCE pool_sequence")
            .await
            .unwrap();
        rows(driver, "SELECT nextval('pool_sequence')").await;
    } else {
        driver
            .run_unprepared("CREATE TEMPORARY TABLE pool_auto (id INT AUTO_INCREMENT PRIMARY KEY)")
            .await
            .unwrap();
        driver
            .run_unprepared("INSERT INTO pool_auto VALUES ()")
            .await
            .unwrap();
    }
    assert_eq!(driver.last_id(None).await.unwrap(), "1");
    driver.run_unprepared("BEGIN").await.unwrap();
    driver
        .run_unprepared("INSERT INTO phera_pool_dirty VALUES (1)")
        .await
        .unwrap();
}

pub async fn clean(driver: &mut Lease, name: &str) {
    let missing = driver
        .execute(
            &Sql::parse("SELECT * FROM phera_pool_dirty".into()).unwrap(),
            &Prepared::Emulated,
            &[],
        )
        .await;
    assert!(missing.is_err(), "temporary table survived reset");
    let (variable, prepared, transaction) = if name == "pgsql" {
        (
            "SELECT current_setting('application_name')",
            "SELECT count(*) FROM pg_prepared_statements WHERE name = 'dirty_statement'",
            "SELECT count(*) FROM pg_stat_activity \
             WHERE pid=pg_backend_pid() AND xact_start IS NOT NULL",
        )
    } else {
        (
            "SELECT @phera_pool_dirty",
            "SELECT 0",
            "SELECT @@session.in_transaction",
        )
    };
    assert_eq!(
        scalar(driver, variable).await,
        if name == "pgsql" {
            Value::from("")
        } else {
            Value::Null
        }
    );
    assert_eq!(scalar(driver, prepared).await, Value::Int(0));
    if name == "pgsql" {
        // Every SELECT has its own implicit transaction; no explicit BEGIN remains.
        driver.run_unprepared("DISCARD ALL").await.unwrap();
        assert!(driver.last_id(None).await.is_err());
    } else {
        // MySQL 8 does not expose MariaDB's @@in_transaction. A new transaction and
        // reset are additionally checked by the lock-release lifecycle integration test.
        let _ = transaction;
        assert!(
            driver
                .run_unprepared("EXECUTE dirty_statement")
                .await
                .is_err()
        );
        assert_eq!(driver.last_id(None).await.unwrap(), "0");
    }
    assert_eq!(rows(driver, "SELECT 1").await.values[0][0], Value::Int(1));
}

/// A clean connection survives being reset many times in a row.
pub async fn reset_repeatedly(driver: &mut Lease) {
    for _ in 0..100 {
        driver.reset().await.unwrap();
    }
    assert_eq!(rows(driver, "SELECT 1").await.values[0][0], Value::Int(1));
}

/// Private tokio-postgres type-info statements cannot survive DISCARD ALL.
pub async fn custom_types_retire_client() {
    let pool = super::TestPool::new("pgsql", super::options(1));
    let mut first = pool.connect().await;
    let original = super::id(&mut first, "pgsql").await;
    first
        .run_unprepared("CREATE TEMP TABLE pool_custom_init(id INT)")
        .await
        .unwrap();
    first
        .run_unprepared("CREATE TYPE pg_temp.pool_mood AS ENUM ('ok')")
        .await
        .unwrap();
    assert_eq!(
        scalar(&mut first, "SELECT 'ok'::pg_temp.pool_mood").await,
        Value::from("ok")
    );
    drop(first);
    let mut second = pool.connect().await;
    assert_ne!(super::id(&mut second, "pgsql").await, original);
    second
        .run_unprepared("CREATE TEMP TABLE pool_custom_init(id INT)")
        .await
        .unwrap();
    second
        .run_unprepared("CREATE TYPE pg_temp.pool_mood AS ENUM ('new')")
        .await
        .unwrap();
    assert_eq!(
        scalar(&mut second, "SELECT 'new'::pg_temp.pool_mood").await,
        Value::from("new")
    );
}
