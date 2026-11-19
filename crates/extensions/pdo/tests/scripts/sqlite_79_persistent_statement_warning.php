<?php
// PHERA_ORACLE_PHP_SINCE: 80513
// php-src 26e5bc454c9 removed the duplicate warning for a persistent handle.
$pdo = new PDO('sqlite::memory:', options: [
    PDO::ATTR_PERSISTENT => true,
    PDO::ATTR_ERRMODE => PDO::ERRMODE_WARNING,
]);
set_error_handler(function ($severity, $message) {
    echo $severity, ': ', $message, "\n";
    return true;
});
var_dump($pdo->setAttribute(PDO::ATTR_STATEMENT_CLASS, [PDOStatement::class]));
