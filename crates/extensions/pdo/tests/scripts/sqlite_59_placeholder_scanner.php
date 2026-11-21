<?php
// Oracle: PHP 8.5.12. SQLite numbers slots in its native prepared statement.
$pdo = new PDO('sqlite::memory:');
$tick = chr(96);
foreach ([
    ['bracket', 'SELECT 1 AS [?:v], ? AS y', ['p']],
    ['backtick', "SELECT 1 AS {$tick}?:v{$tick}, ? AS y", ['p']],
    ['double quote', 'SELECT 1 AS "\?:v", ? AS y', ['p']],
    ['standard string', "SELECT 'a\\' AS x, ? AS y", ['p']],
    ['unterminated literal', "SELECT ? AS y, 'a?", ['p']],
    ['numbered slots', 'SELECT ?2 AS x, ?1 AS y', ['a', 'b']],
] as [$label, $sql, $arguments]) {
    try {
        $statement = $pdo->prepare($sql);
        $statement->execute($arguments);
        echo $label, ' ', json_encode($statement->fetch(PDO::FETCH_ASSOC)), "\n";
    } catch (PDOException $error) {
        echo $label, ' ', $error->getCode(), "\n";
    }
}
