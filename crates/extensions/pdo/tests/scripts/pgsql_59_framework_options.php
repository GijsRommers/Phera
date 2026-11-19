<?php
$pdo = new PDO(
    "pgsql:host=127.0.0.1;port=54329;dbname='phera_test';client_encoding='utf8';sslmode=prefer",
    'phera', 'phera', [PDO::ATTR_TIMEOUT => 5, PDO::ATTR_PREFETCH => false],
);
$statement = $pdo->query("SELECT 'r'::\"char\" AS kind, ''::\"char\" AS empty");
var_dump($statement->fetchAll(PDO::FETCH_ASSOC));
foreach ([false, true] as $disabled) {
    var_dump($pdo->setAttribute(Pdo\Pgsql::ATTR_DISABLE_PREPARES, $disabled));
    var_dump($pdo->getAttribute(Pdo\Pgsql::ATTR_DISABLE_PREPARES));
    $statement = $pdo->prepare('SELECT i FROM generate_series(1, :limit::int) AS i');
    $statement->execute(['limit' => 2]);
    var_dump($statement->rowCount(), $statement->columnCount(), $statement->fetchColumn());
    var_dump($statement->fetchColumn(), $statement->fetchColumn(), $statement->rowCount());
    $statement->execute(['limit' => 3]);
    var_dump($statement->fetchAll(PDO::FETCH_COLUMN), $statement->rowCount());
}
