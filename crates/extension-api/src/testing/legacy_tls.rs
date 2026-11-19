//! Compare rejection of a TLS 1.0-only peer with PHP, for example
//! `new PDO('pgsql:...;sslmode=require;ssl_min_protocol_version=TLSv1.2')`.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use super::oracle::find_php;

const SERVER: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/testing/legacy_tls_server.php"
);

const CONTROL: &str = r#"
$options = ['verify_peer' => false, 'verify_peer_name' => false, 'security_level' => 0];
$context = stream_context_create(['ssl' => $options]);
$socket = stream_socket_client('tcp://' . $argv[1], timeout: 3, context: $context);
stream_set_timeout($socket, 3);
if ($argv[2] === 'pgsql') {
    fwrite($socket, hex2bin('0000000804d2162f'));
    if (fread($socket, 1) !== 'S') exit(2);
}
if (!stream_socket_enable_crypto($socket, true, STREAM_CRYPTO_METHOD_TLSv1_0_CLIENT)) exit(3);
echo fread($socket, 5);
"#;

/// A local OpenSSL-backed server restricted to TLS 1.0. A control client
/// completes a handshake before the callers test the rejection paths.
pub struct LegacyTlsServer {
    php: PathBuf,
    child: Child,
    address: String,
    protocol: String,
}

impl LegacyTlsServer {
    /// `protocol` is `pgsql` for an SSLRequest or `redis` for immediate TLS.
    pub fn start(protocol: &str, directory: &Path) -> Self {
        let php = find_php().expect("PHP oracle available");
        let child = Command::new(&php)
            .args(["-n", SERVER, protocol])
            .arg(directory.join("tls.pem"))
            .stdout(Stdio::piped())
            .spawn()
            .expect("start the TLS 1.0 fixture");
        let mut server = Self {
            php,
            child,
            address: String::new(),
            protocol: protocol.to_owned(),
        };
        BufReader::new(server.child.stdout.take().expect("piped stdout"))
            .read_line(&mut server.address)
            .expect("read TLS fixture address");
        server.address = server.address.trim().to_owned();
        assert!(
            server.address.starts_with("127.0.0.1:"),
            "{}",
            server.address
        );
        assert_eq!(server.php_client(CONTROL), "TLSv1");
        server
    }

    pub fn port(&self) -> u16 {
        self.address.split_once(':').unwrap().1.parse().unwrap()
    }

    /// The PHP snippet receives the address, protocol and port as arguments.
    pub fn php_client(&self, script: &str) -> String {
        let output = Command::new(&self.php)
            .args(["-n", "-r", script, "--", &self.address, &self.protocol])
            .arg(self.port().to_string())
            .output()
            .expect("run the PHP TLS client");
        assert!(
            output.status.success(),
            "PHP TLS client failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("UTF-8 PHP oracle output")
    }
}

impl Drop for LegacyTlsServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
