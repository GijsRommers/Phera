<?php
// Missing prerequisites must fail CI, rather than turn oracle tests into skips.
if (PHP_MAJOR_VERSION !== 8 || PHP_MINOR_VERSION !== 5) {
    throw new RuntimeException('The oracle must be PHP 8.5; found ' . PHP_VERSION);
}
foreach (['json', 'pcre', 'ctype', 'pdo_pgsql', 'pdo_mysql', 'pdo_sqlite', 'sqlite3', 'mbstring', 'iconv', 'tokenizer', 'openssl', 'libxml', 'dom', 'simplexml', 'xmlreader', 'xmlwriter', 'redis'] as $extension) {
    if (!extension_loaded($extension)) {
        throw new RuntimeException('Missing PHP extension: ' . $extension);
    }
}
// The date oracle must use PHP's embedded data, not a distribution's zoneinfo.
// CI supplies timezonedb-2026.5; PHP builds embedding the same version need no module.
if (timezone_version_get() !== '2026.5') {
    throw new RuntimeException(
        'The date oracle requires timezonedb 2026.5; found ' . timezone_version_get()
    );
}
foreach (['pgsql' => 54329, 'mysql' => 33069] as $driver => $port) {
    new PDO("$driver:host=127.0.0.1;port=$port;dbname=phera_test", 'phera', 'phera');
}
// The connection pool tests count the connections the server sees for one
// account, so they use an account of their own that no other test logs in
// with. Create it where it is missing (a fresh CI database has only `phera`).
$postgres = new PDO('pgsql:host=127.0.0.1;port=54329;dbname=phera_test', 'phera', 'phera');
$exists = $postgres->query("SELECT 1 FROM pg_roles WHERE rolname = 'phera_pool'")->fetchColumn();
if (!$exists) {
    $postgres->exec("CREATE ROLE phera_pool LOGIN PASSWORD 'phera_pool'");
}
$postgres->exec('GRANT ALL ON DATABASE phera_test TO phera_pool');
$postgres->exec('GRANT ALL ON SCHEMA public TO phera_pool');

$rootPassword = getenv('PHERA_TEST_MYSQL_ROOT_PASSWORD') ?: 'phera-root';
$mysql = new PDO('mysql:host=127.0.0.1;port=33069', 'root', $rootPassword);
$mysql->exec("CREATE USER IF NOT EXISTS 'phera_pool'@'%' IDENTIFIED BY 'phera_pool'");
$mysql->exec("GRANT ALL ON phera_test.* TO 'phera_pool'@'%'");

foreach (['pgsql' => 54329, 'mysql' => 33069] as $driver => $port) {
    new PDO("$driver:host=127.0.0.1;port=$port;dbname=phera_test", 'phera_pool', 'phera_pool');
}
echo 'PHP ', PHP_VERSION, ': extensions, both test databases and the pool test account ready', PHP_EOL;

$redis = new Redis();
$redis->connect('127.0.0.1', 16399);
$redis->config('SET', ['save' => '', 'appendonly' => 'no']);
$redis->ping();
echo 'phpredis ', phpversion('redis'), ': Redis test server ready', PHP_EOL;
