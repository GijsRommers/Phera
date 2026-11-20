//! A timed-out `pg_query($db, $sql)` must leave the next result's fields intact.

use std::collections::BTreeMap;
use std::time::Duration;

use phera_db_drivers::connect_options::ConnectOptions;
use phera_db_drivers::drivers::Driver;
use phera_db_drivers::dsn::{DriverName, Dsn};
use phera_db_drivers::pool::{self, Policy};

#[tokio::test(flavor = "current_thread")]
async fn cancelled_query_does_not_supply_headers_for_the_next_query() {
    let dsn = Dsn {
        driver: DriverName::Postgres,
        fields: BTreeMap::from([
            ("host".into(), "127.0.0.1".into()),
            ("port".into(), "54329".into()),
            ("dbname".into(), "pgsql_ext".into()),
        ]),
        options: ConnectOptions {
            postgres_native: true,
            ..Default::default()
        },
    };
    let lease = pool::acquire(&dsn, "phera", "phera", Policy::Unpooled)
        .await
        .expect("PostgreSQL test server");
    let slow = lease.run_cancellable(|driver| {
        Box::pin(async {
            let Driver::Postgres(driver) = driver else {
                panic!("expected PostgreSQL");
            };
            driver
                .native_query("SELECT pg_sleep(0.2) AS old_field")
                .await
        })
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(15), slow)
            .await
            .is_err()
    );
    let rows = lease
        .run_cancellable(|driver| {
            Box::pin(async {
                let Driver::Postgres(driver) = driver else {
                    panic!("expected PostgreSQL");
                };
                driver.native_query("SELECT 42 AS new_field").await
            })
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].columns[0].name, "new_field");
    assert_eq!(rows[0].values[0][0].as_deref(), Some(&b"42"[..]));
    assert_eq!(rows[0].command, "SELECT 1");
}
