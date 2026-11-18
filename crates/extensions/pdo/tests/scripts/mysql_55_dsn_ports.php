<?php
// PHP 8.5 oracle: 55 the port of a DSN, read the way mysqlnd reads it (atoi).
// Only spellings of the test server's port: port 0 or an unparsable one would
// fall back to the default port 3306, which belongs to another server.
foreach (['33069', ' 33069', '+33069', '33069abc', '33069 ', '98605'] as $port) {
    try {
        new PDO("mysql:host=127.0.0.1;port=$port;dbname=phera_test", 'phera', 'phera');
        echo var_export($port, true), ": connected\n";
    } catch (PDOException $e) {
        echo var_export($port, true), ': ', var_export($e->getCode(), true), ' ', $e->getMessage(), "\n";
    }
}
