--TEST--
The MySQL, PostgreSQL and SQLite driver constants, on the driver classes and under their old names on PDO
--FILE--
<?php
// The driver constants: on the driver classes, and under their old names on PDO (deprecated in 8.5).
foreach (['Pdo\Mysql', 'Pdo\Pgsql', 'Pdo\Sqlite'] as $class) {
    $own = array_diff_key((new ReflectionClass($class))->getConstants(), (new ReflectionClass('PDO'))->getConstants());
    // Pdo\Pgsql::TRANSACTION_* are deprecated without effect in 8.5 and left out of Phera; see the not-supported fixture.
    $own = array_filter($own, fn($k) => !str_starts_with($k, 'TRANSACTION_') || $class !== 'Pdo\Pgsql', ARRAY_FILTER_USE_KEY);
    ksort($own); echo $class, ' (', count($own), '): ', json_encode($own), "\n";
}
$old = array_filter((new ReflectionClass('PDO'))->getConstants(), fn($k) => (bool) preg_match('/^(MYSQL|PGSQL|SQLITE)_/', $k), ARRAY_FILTER_USE_KEY); ksort($old); echo count($old), ' ', json_encode($old), "\n";
echo Pdo\Mysql::ATTR_SSL_CA, ' ', Pdo\Pgsql::ATTR_DISABLE_PREPARES, ' ', Pdo\Pgsql::ATTR_RESULT_MEMORY_SIZE, "\n";
echo PDO::MYSQL_ATTR_INIT_COMMAND, "\n", PDO::PGSQL_ATTR_DISABLE_PREPARES, "\n", PDO::PGSQL_TRANSACTION_IDLE, "\n", PDO::SQLITE_OPEN_READONLY, "\n";
var_dump(defined('PDO::MYSQL_ATTR_SSL_CA'), defined('Pdo\Mysql::ATTR_SSL_CA'), defined('PDO::ATTR_SSL_CA'), constant('Pdo\Mysql::ATTR_USE_BUFFERED_QUERY'));
// what Laravel's database configuration does
$options = extension_loaded('pdo_mysql') ? array_filter([Pdo\Mysql::ATTR_SSL_CA => null, Pdo\Mysql::ATTR_INIT_COMMAND => 'SET NAMES utf8mb4']) : []; var_dump($options);
--EXPECTF--
Pdo\Mysql (16): {"ATTR_COMPRESS":1003,"ATTR_DIRECT_QUERY":20,"ATTR_FOUND_ROWS":1004,"ATTR_IGNORE_SPACE":1005,"ATTR_INIT_COMMAND":1002,"ATTR_LOCAL_INFILE":1001,"ATTR_LOCAL_INFILE_DIRECTORY":1014,"ATTR_MULTI_STATEMENTS":1012,"ATTR_SERVER_PUBLIC_KEY":1011,"ATTR_SSL_CA":1008,"ATTR_SSL_CAPATH":1009,"ATTR_SSL_CERT":1007,"ATTR_SSL_CIPHER":1010,"ATTR_SSL_KEY":1006,"ATTR_SSL_VERIFY_SERVER_CERT":1013,"ATTR_USE_BUFFERED_QUERY":1000}
Pdo\Pgsql (2): {"ATTR_DISABLE_PREPARES":1000,"ATTR_RESULT_MEMORY_SIZE":1001}
Pdo\Sqlite (19): {"ATTR_BUSY_STATEMENT":1003,"ATTR_EXPLAIN_STATEMENT":1004,"ATTR_EXTENDED_RESULT_CODES":1002,"ATTR_OPEN_FLAGS":1000,"ATTR_READONLY_STATEMENT":1001,"ATTR_TRANSACTION_MODE":1005,"DENY":1,"DETERMINISTIC":2048,"EXPLAIN_MODE_EXPLAIN":1,"EXPLAIN_MODE_EXPLAIN_QUERY_PLAN":2,"EXPLAIN_MODE_PREPARED":0,"IGNORE":2,"OK":0,"OPEN_CREATE":4,"OPEN_READONLY":1,"OPEN_READWRITE":2,"TRANSACTION_MODE_DEFERRED":0,"TRANSACTION_MODE_EXCLUSIVE":2,"TRANSACTION_MODE_IMMEDIATE":1}
29 {"MYSQL_ATTR_COMPRESS":1003,"MYSQL_ATTR_DIRECT_QUERY":20,"MYSQL_ATTR_FOUND_ROWS":1004,"MYSQL_ATTR_IGNORE_SPACE":1005,"MYSQL_ATTR_INIT_COMMAND":1002,"MYSQL_ATTR_LOCAL_INFILE":1001,"MYSQL_ATTR_LOCAL_INFILE_DIRECTORY":1014,"MYSQL_ATTR_MULTI_STATEMENTS":1012,"MYSQL_ATTR_SERVER_PUBLIC_KEY":1011,"MYSQL_ATTR_SSL_CA":1008,"MYSQL_ATTR_SSL_CAPATH":1009,"MYSQL_ATTR_SSL_CERT":1007,"MYSQL_ATTR_SSL_CIPHER":1010,"MYSQL_ATTR_SSL_KEY":1006,"MYSQL_ATTR_SSL_VERIFY_SERVER_CERT":1013,"MYSQL_ATTR_USE_BUFFERED_QUERY":1000,"PGSQL_ATTR_DISABLE_PREPARES":1000,"PGSQL_TRANSACTION_ACTIVE":1,"PGSQL_TRANSACTION_IDLE":0,"PGSQL_TRANSACTION_INERROR":3,"PGSQL_TRANSACTION_INTRANS":2,"PGSQL_TRANSACTION_UNKNOWN":4,"SQLITE_ATTR_EXTENDED_RESULT_CODES":1002,"SQLITE_ATTR_OPEN_FLAGS":1000,"SQLITE_ATTR_READONLY_STATEMENT":1001,"SQLITE_DETERMINISTIC":2048,"SQLITE_OPEN_CREATE":4,"SQLITE_OPEN_READONLY":1,"SQLITE_OPEN_READWRITE":2}
1008 1000 1001

Deprecated: Constant PDO::MYSQL_ATTR_INIT_COMMAND is deprecated since 8.5, use Pdo\Mysql::ATTR_INIT_COMMAND instead in %s on line 11
1002

Deprecated: Constant PDO::PGSQL_ATTR_DISABLE_PREPARES is deprecated since 8.5, use Pdo\Pgsql::ATTR_DISABLE_PREPARES instead in %s on line 11
1000

Deprecated: Constant PDO::PGSQL_TRANSACTION_IDLE is deprecated since 8.5 in %s on line 11
0

Deprecated: Constant PDO::SQLITE_OPEN_READONLY is deprecated since 8.5, use Pdo\Sqlite::OPEN_READONLY instead in %s on line 11
1
bool(true)
bool(true)
bool(false)
int(1000)
array(1) {
  [1002]=>
  string(17) "SET NAMES utf8mb4"
}
