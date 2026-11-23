<?php
// Oracle: PHP 8.5.12. MySQL has separate string, identifier and comment rules.
$pdo = new PDO('mysql:host=127.0.0.1;port=33069;dbname=phera_test', 'phera', 'phera');
function probe(PDO $pdo, string $label, string $sql, array $arguments): void {
    try {
        $statement = $pdo->prepare($sql);
        $statement->execute($arguments);
        echo $label, ' ', json_encode($statement->fetch(PDO::FETCH_ASSOC)), "\n";
    } catch (PDOException $error) {
        echo $label, ' ', $error->getCode(), "\n";
    }
}

foreach ([true, false] as $emulate) {
    $pdo->setAttribute(PDO::ATTR_EMULATE_PREPARES, $emulate);
    $pdo->exec("SET SESSION sql_mode=''");
    echo $emulate ? "emulated\n" : "native\n";
    probe($pdo, 'escaped single quote', "SELECT 'a\\'b' AS x, ? AS y", ['p']);
    probe($pdo, 'double-quoted string', 'SELECT "a\"b" AS x, ? AS y', ['p']);
    $tick = chr(96);
    probe($pdo, 'backtick identifier', "SELECT 1 AS {$tick}?:v{$tick}, ? AS y", ['p']);
    probe($pdo, 'hash comment', "SELECT 1 AS x # ? :v\n, ? AS y", ['p']);
    probe($pdo, 'minus followed by minus', 'SELECT 4--? AS x', [3]);
    probe($pdo, 'non-nesting block comment', 'SELECT 1 AS x /* outer /* ? */ , ? AS y', ['p']);
    probe($pdo, 'unterminated literal', "SELECT ? AS y, 'a?", ['p']);
    if ($emulate) {
        probe($pdo, 'only an unterminated literal', "SELECT 'a?", ['p']);
    }
}
