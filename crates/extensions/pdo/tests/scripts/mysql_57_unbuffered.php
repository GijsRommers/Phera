<?php
$pdo = new PDO('mysql:host=127.0.0.1;port=33069;dbname=phera_test', 'phera', 'phera');
foreach ([false, true] as $emulate) {
    $pdo->setAttribute(PDO::ATTR_EMULATE_PREPARES, $emulate);
    var_dump($pdo->setAttribute(Pdo\Mysql::ATTR_USE_BUFFERED_QUERY, false));
    var_dump($pdo->getAttribute(Pdo\Mysql::ATTR_USE_BUFFERED_QUERY));
    $statement = $pdo->query('SELECT 1 AS n UNION ALL SELECT 2');
    var_dump($statement->rowCount(), $statement->columnCount(), $statement->fetchColumn());
    try {
        $pdo->query('SELECT 3');
    } catch (PDOException $e) {
        echo $e->getMessage(), "\n";
    }
    var_dump($statement->fetchAll(PDO::FETCH_COLUMN), $statement->rowCount());
    $statement = $pdo->query('SELECT 3');
    var_dump($statement->closeCursor(), $pdo->query('SELECT 4')->fetchAll(PDO::FETCH_COLUMN));
    $statement = $pdo->query('SELECT 5');
    unset($statement);
    var_dump($pdo->query('SELECT 6')->fetchAll(PDO::FETCH_COLUMN));
}
