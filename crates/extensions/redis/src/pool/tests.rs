//! Live tests use only the task's Redis container, own database and unique keys.
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

async fn isolated(size: usize) -> Arc<Bucket<Socket>> {
    runtime()
        .await
        .unwrap()
        .spawn(async move {
            Bucket::new(phera_io_pool::Options {
                size,
                ..Default::default()
            })
        })
        .await
        .unwrap()
}

async fn lease(bucket: &Arc<Bucket<Socket>>) -> Lease {
    let bucket = bucket.clone();
    runtime()
        .await
        .unwrap()
        .spawn(async move {
            let permit = bucket.capacity.clone().acquire_owned().await.unwrap();
            borrow(bucket, permit, endpoint()).await.unwrap()
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn reuse_and_cleanup_after_multi_watch_database_and_client_name() {
    let bucket = isolated(1).await;
    let first = lease(&bucket).await;
    let original = id(&first).await;
    for args in [
        vec!["SELECT", "14"],
        vec!["CLIENT", "SETNAME", "dirty"],
        vec!["WATCH", "phera-pool-watch"],
        vec!["MULTI"],
    ] {
        let mut command = redis::cmd(args[0]);
        command.arg(&args[1..]);
        first.query(command, 2.0).await.unwrap();
    }
    drop(first);
    let second = lease(&bucket).await;
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
    let bucket = isolated(1).await;
    let first = lease(&bucket).await;
    let original = id(&first).await;
    assert!(
        timeout(
            Duration::from_millis(30),
            bucket.capacity.clone().acquire_owned()
        )
        .await
        .is_err()
    );
    let killer = acquire(endpoint()).await.unwrap();
    let mut kill = redis::cmd("CLIENT");
    kill.arg("KILL").arg("ID").arg(original);
    killer.query(kill, 2.0).await.unwrap();
    drop(first);
    let second = lease(&bucket).await;
    assert_ne!(id(&second).await, original);
    second.retire().await.unwrap();
    let retired = id(&second).await;
    drop(second);
    assert_ne!(id(&lease(&bucket).await).await, retired);
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
    let bucket = isolated(1).await;
    let owner = bucket.clone();
    let first = runtime()
        .await
        .unwrap()
        .spawn(async move {
            let permit = owner.capacity.clone().acquire_owned().await.unwrap();
            let client = endpoint().client().unwrap();
            let info = client.get_connection_info().clone();
            let settings = info
                .redis_settings()
                .clone()
                .set_protocol(redis::ProtocolVersion::RESP3);
            let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
            let connection = redis::Client::open(info.set_redis_settings(settings))
                .unwrap()
                .get_multiplexed_async_connection_with_config(
                    &redis::AsyncConnectionConfig::new()
                        .set_push_sender(sender)
                        .set_response_timeout(None),
                )
                .await
                .unwrap();
            Lease(Arc::new(Shared {
                socket: Mutex::new(Some(Socket {
                    connection,
                    reusable: true,
                })),
                owner: Some(ReturnTo {
                    bucket: owner,
                    permit,
                    born: Instant::now(),
                }),
                runtime: tokio::runtime::Handle::current(),
            }))
        })
        .await
        .unwrap();
    let mut subscribe = redis::cmd("SUBSCRIBE");
    subscribe.arg(format!("phera:pool:subscribe:{}", std::process::id()));
    first.query(subscribe, 2.0).await.unwrap();
    drop(first);
    let second = lease(&bucket).await;
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
