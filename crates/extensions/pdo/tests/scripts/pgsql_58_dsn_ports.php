<?php
// PHP 8.5 oracle: 58 the port of a DSN, read the way libpq reads it.
foreach (['54329', ' 54329 ', '+54329', 'abc', '54329abc', '0', '-1', '65536', '99999', '2147483648'] as $port) {
    try {
        new PDO("pgsql:host=127.0.0.1;port=$port;dbname=phera_test", 'phera', 'phera');
        echo var_export($port, true), ": connected\n";
    } catch (PDOException $e) {
        echo var_export($port, true), ': ', var_export($e->getCode(), true), ' ', $e->getMessage(), "\n";
    }
}
