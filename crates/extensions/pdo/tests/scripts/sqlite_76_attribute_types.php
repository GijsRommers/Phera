<?php
$pdo = new PDO('sqlite::memory:');
$attributes = [PDO::ATTR_ORACLE_NULLS, PDO::ATTR_DEFAULT_FETCH_MODE, PDO::ATTR_STRINGIFY_FETCHES];
foreach ($attributes as $a) {
    $values = [null, false, true, ' 1 ', '1e0', 1.5, [], [9], new stdClass];
    if ($a !== PDO::ATTR_ORACLE_NULLS) {
        $values = array_merge($values, [-1, 257, PHP_INT_MAX]);
    }
    foreach ($values as $value) {
        try {
            var_dump($pdo->setAttribute($a, $value), $pdo->getAttribute($a));
        } catch (Throwable $failure) {
            echo get_class($failure), ': ', $failure->getMessage(), "\n";
        }
    }
}
