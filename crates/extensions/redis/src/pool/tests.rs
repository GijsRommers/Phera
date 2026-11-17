//! Live tests against the dedicated Redis container (port 16399), each with
//! its own pool, keys and persistent id.
use super::*;
use std::time::Duration;

fn endpoint() -> Endpoint {
    Endpoint {
        host: "127.0.0.1".into(),
        port: 16399,
        timeout_micros: 2_000_000,
        persistent_id: Some(format!("pool-test-{}", std::process::id())),
    }
}

async fn id(lease: &Lease) -> i64 {
    let mut command = redis::cmd("CLIENT");
    command.arg("ID");
    match lease.query(command, 2.0).await.unwrap() {
        Value::Int(id) => id,
        value => panic!("unexpected id {value:?}"),
    }
}

/// A pool of its own with `size` sockets, on the process's I/O runtime.
fn isolated(size: usize) -> Pool<Endpoint, Socket> {
    Pool::with_options(
        &RUNTIME,
        Options {
            size,
            ..POOL.options().unwrap()
        },
    )
}

async fn lease(pool: &Pool<Endpoint, Socket>) -> Lease {
    acquire_from(pool, endpoint()).await.unwrap()
}

#[tokio::test]
async fn reuse_and_cleanup_after_multi_watch_database_and_client_name() {
    let pool = isolated(1);
    let first = lease(&pool).await;
    let original = id(&first).await;
    let watched = format!("phera:pool:watch:{}", std::process::id());
    for args in [
        vec!["SELECT", "14"],
        vec!["CLIENT", "SETNAME", "dirty"],
        vec!["WATCH", &watched],
        vec!["MULTI"],
    ] {
        let mut command = redis::cmd(args[0]);
        command.arg(&args[1..]);
        first.query(command, 2.0).await.unwrap();
    }
    drop(first);
    let second = lease(&pool).await;
    assert_eq!(id(&second).await, original);
    let mut name = redis::cmd("CLIENT");
    name.arg("GETNAME");
    assert!(matches!(second.query(name, 2.0).await.unwrap(), Value::Nil));
    let mut info = redis::cmd("CLIENT");
    info.arg("INFO");
    let Value::BulkString(info) = second.query(info, 2.0).await.unwrap() else {
        panic!("info");
    };
    assert!(String::from_utf8_lossy(&info).contains("db=0"));
    assert!(matches!(
        second.query(redis::cmd("PING"), 2.0).await.unwrap(),
        Value::SimpleString(_)
    ));
}

#[tokio::test]
async fn limit_and_killed_connection_replacement() {
    let pool = isolated(1);
    let first = lease(&pool).await;
    let original = id(&first).await;
    assert!(
        timeout(Duration::from_millis(30), lease(&pool))
            .await
            .is_err(),
        "a second borrower waits while the only socket is lent"
    );
    let killer = acquire(endpoint()).await.unwrap();
    let mut kill = redis::cmd("CLIENT");
    kill.arg("KILL").arg("ID").arg(original);
    killer.query(kill, 2.0).await.unwrap();
    drop(first);
    let second = lease(&pool).await;
    assert_ne!(id(&second).await, original);
    second.retire().await.unwrap();
    let retired = id(&second).await;
    drop(second);
    assert_ne!(id(&lease(&pool).await).await, retired);
}

#[test]
fn connection_survives_its_opening_worker() {
    let connection = std::thread::spawn(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(acquire(endpoint()))
            .unwrap()
    })
    .join()
    .unwrap();
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                assert!(id(&connection).await > 0);
            })
    })
    .join()
    .unwrap();
}

#[tokio::test]
async fn subscribed_socket_is_cleaned_before_reuse() {
    let pool = isolated(1);
    // A socket that receives pushes, as a subscription would use.
    let subscriber = async {
        let client = endpoint().client()?;
        let info = client.get_connection_info().clone();
        let settings = info
            .redis_settings()
            .clone()
            .set_protocol(redis::ProtocolVersion::RESP3);
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
        let connection = redis::Client::open(info.set_redis_settings(settings))?
            .get_multiplexed_async_connection_with_config(
                &redis::AsyncConnectionConfig::new()
                    .set_push_sender(sender)
                    .set_response_timeout(None),
            )
            .await?;
        Ok(Socket {
            connection,
            reusable: true,
        })
    };
    let first = Lease(pool.acquire(endpoint(), subscriber).await.unwrap());
    let mut subscribe = redis::cmd("SUBSCRIBE");
    subscribe.arg(format!("phera:pool:subscribe:{}", std::process::id()));
    first.query(subscribe, 2.0).await.unwrap();
    drop(first);
    let second = lease(&pool).await;
    let mut info = redis::cmd("CLIENT");
    info.arg("INFO");
    let Value::BulkString(info) = second.query(info, 2.0).await.unwrap() else {
        panic!("info");
    };
    assert!(String::from_utf8_lossy(&info).contains("sub=0"));
    assert!(matches!(
        second.query(redis::cmd("PING"), 2.0).await.unwrap(),
        Value::SimpleString(_)
    ));
}
