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
