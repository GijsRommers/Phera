<?php
// Missing prerequisites must fail CI, rather than turn oracle tests into skips.
if (PHP_MAJOR_VERSION !== 8 || PHP_MINOR_VERSION !== 5) {
    throw new RuntimeException('The oracle must be PHP 8.5; found ' . PHP_VERSION);
}
foreach (['json', 'pcre', 'ctype', 'curl', 'pdo_pgsql', 'pdo_mysql', 'mysqli', 'pgsql', 'pdo_sqlite', 'sqlite3', 'mbstring', 'iconv', 'tokenizer', 'openssl', 'sodium', 'libxml', 'dom', 'intl', 'posix', 'bcmath', 'exif', 'fileinfo', 'gd', 'gmp', 'zlib', 'redis'] as $extension) {
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

// Native API oracle tests use separate databases so they can run alongside PDO.
if (getenv('GITHUB_ACTIONS') === 'true') {
    $mysql->exec('CREATE DATABASE IF NOT EXISTS mysqli_ext');
    $mysql->exec("GRANT ALL ON mysqli_ext.* TO 'phera'@'%'");
    if (!$postgres->query("SELECT 1 FROM pg_database WHERE datname = 'pgsql_ext'")->fetchColumn()) {
        $postgres->exec('CREATE DATABASE pgsql_ext');
    }
}
mysqli_report(MYSQLI_REPORT_ERROR | MYSQLI_REPORT_STRICT);
$mysqli = mysqli_connect('127.0.0.1', 'phera', 'phera', 'mysqli_ext', 33069);
mysqli_close($mysqli);
$pgsql = pg_connect('host=127.0.0.1 port=54329 dbname=pgsql_ext user=phera password=phera');
if ($pgsql === false) {
    throw new RuntimeException('Cannot connect to the pgsql test database');
}
pg_close($pgsql);

foreach (['pgsql' => 54329, 'mysql' => 33069] as $driver => $port) {
    new PDO("$driver:host=127.0.0.1;port=$port;dbname=phera_test", 'phera_pool', 'phera_pool');
}
echo 'PHP ', PHP_VERSION, ': extensions, both test databases and the pool test account ready', PHP_EOL;
echo 'intl ICU ', INTL_ICU_VERSION, ' (data ', INTL_ICU_DATA_VERSION, ')', PHP_EOL;

$redis = new Redis();
$redis->connect('127.0.0.1', 16399);
$redis->config('SET', ['save' => '', 'appendonly' => 'no']);
$redis->ping();
echo 'phpredis ', phpversion('redis'), ': Redis test server ready', PHP_EOL;
