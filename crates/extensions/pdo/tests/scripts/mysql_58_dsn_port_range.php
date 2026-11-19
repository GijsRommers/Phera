<?php
// PHERA_ORACLE_PHP_SINCE: 80513
// php-src bad5aaca7ae rejects ports that previously wrapped to a 16-bit value.
$port = '98605';
try {
    new PDO("mysql:host=127.0.0.1;port=$port;dbname=phera_test", 'phera', 'phera');
    echo var_export($port, true), ": connected\n";
} catch (PDOException $e) {
    echo var_export($port, true), ': ', var_export($e->getCode(), true), ' ',
        $e->getMessage(), "\n";
}
