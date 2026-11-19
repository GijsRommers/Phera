<?php
$pdo = new PDO('sqlite::memory:');
$attributes = [PDO::ATTR_ORACLE_NULLS, PDO::ATTR_DEFAULT_FETCH_MODE, PDO::ATTR_STRINGIFY_FETCHES];
$values = [null, false, true, -1, 257, PHP_INT_MAX, ' 1 ', '1e0', 1.5, [], [9], new stdClass];
foreach ($attributes as $a) {
    foreach ($values as $value) {
        try {
            var_dump($pdo->setAttribute($a, $value), $pdo->getAttribute($a));
        } catch (Throwable $failure) {
            echo get_class($failure), ': ', $failure->getMessage(), "\n";
        }
    }
}
foreach ([PDO::ATTR_ERRMODE, PDO::ATTR_CASE] as $a) {
    try {
        new PDO('sqlite::memory:', options: [$a => 3]);
    } catch (ValueError $failure) {
        echo $failure->getMessage(), "\n";
    }
}
