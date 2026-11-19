<?php
// PHERA_ORACLE_PHP_SINCE: 80513
// php-src bba94675f51 validates null modes before saving the value.
// The PHP-8.5 merge 6947fe15b39 includes argument names in the diagnostics.
$pdo = new PDO('sqlite::memory:', options: [PDO::ATTR_ORACLE_NULLS => PDO::NULL_TO_STRING]);
foreach ([9, -1, 257, PHP_INT_MAX] as $mode) {
    try {
        var_dump($pdo->setAttribute(PDO::ATTR_ORACLE_NULLS, $mode));
    } catch (ValueError $failure) {
        echo $failure->getMessage(), "\n";
    }
    var_dump($pdo->getAttribute(PDO::ATTR_ORACLE_NULLS));
    var_dump($pdo->query('SELECT NULL')->fetchColumn());
    try {
        new PDO('sqlite::memory:', options: [PDO::ATTR_ORACLE_NULLS => $mode]);
    } catch (ValueError $failure) {
        echo $failure->getMessage(), "\n";
    }
}
try {
    PDO::connect('sqlite::memory:', options: [PDO::ATTR_ORACLE_NULLS => 3]);
} catch (ValueError $failure) {
    echo $failure->getMessage(), "\n";
}
