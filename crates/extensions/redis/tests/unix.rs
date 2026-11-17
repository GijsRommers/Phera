//! A task-owned Unix socket forwards to the same Redis container for transport parity.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unix_socket_matches_php() {
    use phera_extension_api::testing::oracle::find_php;
    use std::path::PathBuf;
    use std::process::Command;
    let Ok(binary) = std::env::var("PHERA_BIN") else {
        return;
    };
    let php = find_php().expect("PHP oracle");
    let socket = PathBuf::from(format!("/tmp/phera-redis-{}.sock", std::process::id()));
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let proxy = tokio::spawn(async move {
        loop {
            let (mut downstream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut upstream = tokio::net::TcpStream::connect("127.0.0.1:16399")
                    .await
                    .unwrap();
                let _ = tokio::io::copy_bidirectional(&mut downstream, &mut upstream).await;
            });
        }
    });
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../../target/redis-unix-{}.php",
        std::process::id()
    ));
    std::fs::write(
        &script,
        format!(
            r#"<?php
$r=new Redis();var_dump($r->connect('{}',0));var_dump($r->getPort(),$r->ping());
var_dump($r->close(),$r->isConnected());
"#,
            socket.display()
        ),
    )
    .unwrap();
    let outputs = tokio::task::spawn_blocking(move || {
        [php, PathBuf::from(binary)]
            .map(|binary| Command::new(binary).arg(&script).output().unwrap())
    })
    .await
    .unwrap();
    proxy.abort();
    std::fs::remove_file(socket).unwrap();
    assert!(outputs.iter().all(|o| o.status.success()));
    assert_eq!(outputs[0].stdout, outputs[1].stdout);
}
