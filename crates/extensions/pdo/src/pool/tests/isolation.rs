//! Reset must remove each previous borrower's server-side state.

use std::time::Instant;

use phera_extension_api::Value;

use super::{rows, scalar};
use crate::drivers::Prepared;
use crate::placeholders::Sql;
use crate::pool::Lease;

pub async fn dirty(driver: &mut Lease, name: &str) {
    driver
        .direct("CREATE TEMPORARY TABLE phera_pool_dirty (id INT)")
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
    driver.direct(variable).await.unwrap();
    driver.direct(prepare).await.unwrap();
    if name == "pgsql" {
        driver
            .direct("CREATE TEMP SEQUENCE pool_sequence")
            .await
            .unwrap();
        rows(driver, "SELECT nextval('pool_sequence')").await;
    } else {
        driver
            .direct("CREATE TEMPORARY TABLE pool_auto (id INT AUTO_INCREMENT PRIMARY KEY)")
            .await
            .unwrap();
        driver
            .direct("INSERT INTO pool_auto VALUES ()")
            .await
            .unwrap();
    }
    assert_eq!(driver.last_id(None).await.unwrap(), "1");
    driver.direct("BEGIN").await.unwrap();
    driver
        .direct("INSERT INTO phera_pool_dirty VALUES (1)")
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
        driver.direct("DISCARD ALL").await.unwrap();
        assert!(driver.last_id(None).await.is_err());
    } else {
        // MySQL 8 does not expose MariaDB's @@in_transaction. A new transaction and
        // reset are additionally checked by the lock-release lifecycle integration test.
        let _ = transaction;
        assert!(driver.direct("EXECUTE dirty_statement").await.is_err());
        assert_eq!(driver.last_id(None).await.unwrap(), "0");
    }
    assert_eq!(rows(driver, "SELECT 1").await.values[0][0], Value::Int(1));
}

pub async fn reset_cost(driver: &mut Lease, name: &str) {
    let started = Instant::now();
    for _ in 0..100 {
        driver.reset().await.unwrap();
    }
    eprintln!(
        "{name}: 100 clean resets: {:?}; mean {:?}",
        started.elapsed(),
        started.elapsed() / 100
    );
}

/// Private tokio-postgres type-info statements cannot survive DISCARD ALL.
pub async fn custom_types_retire_client() {
    let pool = super::TestPool::new("pgsql", super::options(1)).await;
    let mut first = pool.connect().await;
    let original = super::id(&mut first).await;
    first
        .direct("CREATE TEMP TABLE pool_custom_init(id INT)")
        .await
        .unwrap();
    first
        .direct("CREATE TYPE pg_temp.pool_mood AS ENUM ('ok')")
        .await
        .unwrap();
    assert_eq!(
        scalar(&mut first, "SELECT 'ok'::pg_temp.pool_mood").await,
        Value::from("ok")
    );
    drop(first);
    let mut second = pool.connect().await;
    assert_ne!(super::id(&mut second).await, original);
    second
        .direct("CREATE TEMP TABLE pool_custom_init(id INT)")
        .await
        .unwrap();
    second
        .direct("CREATE TYPE pg_temp.pool_mood AS ENUM ('new')")
        .await
        .unwrap();
    assert_eq!(
        scalar(&mut second, "SELECT 'new'::pg_temp.pool_mood").await,
        Value::from("new")
    );
}
