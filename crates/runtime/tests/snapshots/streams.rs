//! A worker can register `virtual://` and `example.filter` before `Phera\snapshot()`.

use crate::boot_and_answer;

const REGISTER: &str = r#"<?php
class VirtualStream {
    public $context;
    private bool $read = false;
    public function stream_open($path, $mode, $flags, &$opened) { return true; }
    public function stream_read($size) {
        if ($this->read) return '';
        $this->read = true;
        return 'body';
    }
    public function stream_eof() { return $this->read; }
    public function stream_stat() { return ['size' => 4]; }
}
class BootFilter extends php_user_filter {
    public function filter($in, $out, &$consumed, bool $closing): int {
        while ($bucket = stream_bucket_make_writeable($in)) {
            $consumed += $bucket->datalen;
            $bucket->data = strtoupper($bucket->data);
            stream_bucket_append($out, $bucket);
        }
        return PSFS_PASS_ON;
    }
}
stream_wrapper_register('virtual', VirtualStream::class);
stream_filter_register('boot-filter', BootFilter::class);
"#;

#[tokio::test(flavor = "current_thread")]
async fn forks_get_independent_stream_registrations() {
    let script = format!(
        "{REGISTER}{}",
        r#"
Phera\snapshot();
echo file_get_contents('virtual://example');
$stream = fopen('php://memory', 'w+');
fwrite($stream, 'filtered');
rewind($stream);
stream_filter_append($stream, 'boot-filter', STREAM_FILTER_READ);
echo stream_get_contents($stream);
fclose($stream);
var_dump(stream_wrapper_unregister('virtual'));
"#
    );
    let answers = boot_and_answer("wrapper-forks", &script, &["/index.php", "/index.php"])
        .await
        .expect("snapshot");
    assert_eq!(
        answers,
        ["bodyFILTEREDbool(true)\n", "bodyFILTEREDbool(true)\n"]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn snapshots_name_the_open_wrapper_or_filter() {
    for (suffix, operation, reason) in [
        (
            "wrapper",
            "$handle = fopen('virtual://example', 'r');",
            "open user wrapper VirtualStream for virtual://example",
        ),
        (
            "filter",
            "$handle = fopen('php://memory', 'w+'); stream_filter_append($handle, 'boot-filter');",
            "BootFilter",
        ),
    ] {
        let script = format!("{REGISTER}{operation}\nPhera\\snapshot();");
        let failure = boot_and_answer(suffix, &script, &[])
            .await
            .expect_err("snapshot must refuse a live handle");
        assert!(failure.contains(reason), "{failure}");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn snapshots_refuse_live_stream_sockets() {
    let failure = boot_and_answer(
        "stream-socket",
        r#"<?php
$listener = stream_socket_server('tcp://127.0.0.1:0');
Phera\snapshot();
"#,
        &[],
    )
    .await
    .expect_err("snapshot must refuse a live socket");
    assert!(failure.contains("open stream socket"), "{failure}");
}

#[tokio::test(flavor = "current_thread")]
async fn persistent_streams_reopen_in_forks_and_reuse_only_clean_sockets() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::time::{Duration, timeout};

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listen");
    let endpoint = listener.local_addr().expect("address");
    let site = crate::Site::new("persistent-streams");
    let script = site.root.join("index.php");
    std::fs::write(
        &script,
        format!(
            r#"<?php
$socket = pfsockopen('{endpoint}');
stream_set_timeout($socket, 2);
Phera\snapshot();
$same = pfsockopen('{endpoint}');
echo $same === $socket ? 'same ' : 'other ';
fwrite($socket, $_GET['cmd'] . "\n");
echo fgets($socket);
"#
        ),
    )
    .expect("write front controller");
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let worker = phera_runtime::Runtime::new().web_worker(0);
            let boot = worker.boot(&script, &site.root).await;
            assert!(matches!(boot, phera_runtime::BootReport::Snapshot { .. }));
            assert!(
                timeout(Duration::from_millis(30), listener.accept())
                    .await
                    .is_err()
            );

            let peer = tokio::spawn(async move {
                let mut connection_id = 0;
                while let Ok((socket, _)) = listener.accept().await {
                    connection_id += 1;
                    tokio::spawn(async move {
                        let mut socket = BufReader::new(socket);
                        let mut command = String::new();
                        while socket
                            .read_line(&mut command)
                            .await
                            .is_ok_and(|count| count > 0)
                        {
                            let packet = if command.trim_end() == "dirty" {
                                format!("{connection_id}\nunread\n")
                            } else {
                                format!("{connection_id}\n")
                            };
                            if socket.get_mut().write_all(packet.as_bytes()).await.is_err() {
                                break;
                            }
                            if command.trim_end() == "drop" {
                                socket.get_mut().shutdown().await.expect("close the peer");
                                break;
                            }
                            command.clear();
                        }
                    });
                }
            });
            for (command, expected) in [
                ("get", "same 1\n"),
                ("get", "same 1\n"),
                ("dirty", "same 1\n"),
                ("drop", "same 2\n"),
                ("get", "same 3\n"),
            ] {
                let path = format!("/index.php?cmd={command}");
                let response = worker
                    .handle_request(&script, &site.root, crate::request(&path))
                    .await;
                assert_eq!(response.body, expected.as_bytes());
            }
            peer.abort();
        })
        .await;
}
