//! Interrupting `pg_query($db, $sql)` retires the unpooled socket.

use std::collections::BTreeMap;
use std::time::Duration;

use phera_db_drivers::connect_options::ConnectOptions;
use phera_db_drivers::drivers::Driver;
use phera_db_drivers::dsn::{DriverName, Dsn};
use phera_db_drivers::pool::{self, Policy};

#[tokio::test(flavor = "current_thread")]
async fn interrupted_unpooled_query_closes_its_socket() {
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
    let slow = lease.run(async |driver| {
        let Driver::Postgres(driver) = driver else {
            panic!("expected PostgreSQL");
        };
        driver
            .native_query("SELECT pg_sleep(0.2) AS old_field")
            .await
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(15), slow)
            .await
            .is_err()
    );
    assert!(lease.try_inspect(|_| ()).is_none());
    let lease = pool::acquire(&dsn, "phera", "phera", Policy::Unpooled)
        .await
        .expect("replacement PostgreSQL socket");
    let rows = lease
        .run(async |driver| {
            let Driver::Postgres(driver) = driver else {
                panic!("expected PostgreSQL");
            };
            driver.native_query("SELECT 42 AS new_field").await
        })
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].columns[0].name, "new_field");
    assert_eq!(rows[0].values[0][0].as_deref(), Some(&b"42"[..]));
    assert_eq!(rows[0].command, "SELECT 1");
}
