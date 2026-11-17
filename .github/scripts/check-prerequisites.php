<?php
// Missing prerequisites must fail CI, rather than turn oracle tests into skips.
if (PHP_MAJOR_VERSION !== 8 || PHP_MINOR_VERSION !== 5) {
    throw new RuntimeException('The oracle must be PHP 8.5; found ' . PHP_VERSION);
}
foreach (['json', 'pcre', 'ctype', 'pdo_pgsql', 'pdo_mysql', 'mbstring'] as $extension) {
    if (!extension_loaded($extension)) {
        throw new RuntimeException('Missing PHP extension: ' . $extension);
    }
}
foreach (['pgsql' => 54329, 'mysql' => 33069] as $driver => $port) {
    new PDO("$driver:host=127.0.0.1;port=$port;dbname=phera_test", 'phera', 'phera');
}
echo 'PHP ', PHP_VERSION, ': extensions and both test databases ready', PHP_EOL;
