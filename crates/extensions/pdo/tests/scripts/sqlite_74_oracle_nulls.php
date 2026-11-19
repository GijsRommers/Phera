<?php
$pdo = new PDO('sqlite::memory:', options: [PDO::ATTR_ORACLE_NULLS => PDO::NULL_TO_STRING]);
var_dump($pdo->getAttribute(PDO::ATTR_ORACLE_NULLS), $pdo->query('SELECT NULL')->fetchColumn());
foreach ([PDO::NULL_NATURAL, PDO::NULL_EMPTY_STRING, PDO::NULL_TO_STRING, 9, -1, 257] as $mode) {
    var_dump($pdo->setAttribute(PDO::ATTR_ORACLE_NULLS, $mode));
    var_dump($pdo->getAttribute(PDO::ATTR_ORACLE_NULLS));
    foreach ([false, true] as $stringify) {
        $pdo->setAttribute(PDO::ATTR_STRINGIFY_FETCHES, $stringify);
        foreach ([PDO::FETCH_ASSOC, PDO::FETCH_NUM, PDO::FETCH_BOTH, PDO::FETCH_OBJ] as $fetch) {
            echo json_encode($pdo->query("SELECT NULL AS n, '' AS e, 0 AS z")->fetch($fetch)), "\n";
        }
        var_dump($pdo->query("SELECT ''")->fetchColumn());
        var_dump($pdo->query("SELECT NULL, ''")->fetchAll(PDO::FETCH_KEY_PAIR));
        var_dump($pdo->query("SELECT '', NULL")->fetchAll(PDO::FETCH_NUM | PDO::FETCH_GROUP));
        var_dump($pdo->query("SELECT NULL AS n, '' AS n")->fetch(PDO::FETCH_NAMED));
    }
}
try {
    $pdo->setAttribute(PDO::ATTR_ORACLE_NULLS, 1.5);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
