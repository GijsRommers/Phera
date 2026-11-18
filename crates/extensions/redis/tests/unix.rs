//! `connect('/path/to/socket')` over a Unix socket, compared with phpredis.
//! The test owns the socket and forwards it to the test Redis server.

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unix_socket_matches_php() {
    use phera_extension_api::testing::scripts::{Comparison, Runtime};

    let Some(comparison) = Comparison::from_environment("redis-unix") else {
        return;
    };
    // Not in the temporary directory, whose path may exceed the 104 bytes a
    // Unix socket path may have on macOS.
    let socket = std::path::PathBuf::from(format!("/tmp/phera-redis-{}.sock", std::process::id()));
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
    let script = comparison.artifacts.join("unix.php");
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
    // The scripts block this thread; the proxy runs on the other one.
    let [php, phera] = tokio::task::block_in_place(|| {
        [Runtime::Php, Runtime::Phera]
            .map(|runtime| comparison.command(runtime, &script).output().unwrap())
    });
    proxy.abort();
    std::fs::remove_file(socket).unwrap();
    comparison.check("unix", &php, &phera).unwrap();
}
