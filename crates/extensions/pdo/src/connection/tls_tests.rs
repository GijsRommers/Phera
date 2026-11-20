//! `new PDO($dsn)` reports the PHP exception for a TLS 1.0-only server.

use phera_extension_api::testing::TestContext;
use phera_extension_api::testing::legacy_tls::LegacyTlsServer;
use phera_extension_api::{Context, PhpArray, Value};

#[tokio::test]
async fn rejects_a_server_restricted_to_tls_1_0() {
    let directory = tempfile::tempdir().unwrap();
    let server = LegacyTlsServer::start("pgsql", directory.path());
    let php = server.php_client(
        r#"
        try {
            new PDO('pgsql:host=127.0.0.1;port=' . $argv[3] .
                ';sslmode=require;ssl_min_protocol_version=TLSv1.2;connect_timeout=3');
        } catch (PDOException $e) {
            echo $e::class, "\n", var_export($e->getCode(), true), "\n", $e->getMessage();
        }
        "#,
    );
    let port = server.port();
    let dsn = format!("pgsql:host=127.0.0.1;port={port};sslmode=require");
    let mut context = TestContext::default();
    let Value::Object(pdo) = context.create_object("PDO", PhpArray::new()).unwrap() else {
        panic!("expected a PDO object");
    };
    let error = crate::connection::construct(&mut context, Some(pdo), vec![Value::from(dsn)])
        .await
        .expect_err("TLS 1.0 must be rejected");
    let (class, message, code, _) = error.into_exception_parts().unwrap();
    let Value::Int(code) = code else {
        panic!("expected an integer PDOException code");
    };
    // Some libpq builds include an extra hint about the allowed protocol range.
    let actual = format!("{class}\n{code}\n{message}");
    let expected = php.lines().take(3).collect::<Vec<_>>().join("\n");
    assert_eq!(actual, expected);
}
