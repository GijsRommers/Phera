<?php
// Oracle: PHP 8.5.12. Check rendered SQL results and binding error states.
$pdo = new PDO('pgsql:host=127.0.0.1;port=54329;dbname=phera_test', 'phera', 'phera');
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
    $pdo->exec('SET standard_conforming_strings TO on');
    echo $emulate ? "emulated\n" : "native\n";
    probe($pdo, 'backslash ends standard literal', "SELECT 'a\\' AS x, ? AS y", ['p']);
    probe($pdo, 'named marker inside a literal', "SELECT 'a\\' AS x, ':v' AS y", [':v' => 'X']);
    probe($pdo, 'escape string', "SELECT E'a\\'b' AS x, ? AS y", ['p']);
    probe($pdo, 'dollar quote', 'SELECT $q$ ? :v $q$ AS x, ? AS y', ['p']);
    probe($pdo, 'cast', "SELECT '1'::int AS x, ? AS y", ['p']);
    probe($pdo, 'doubled question', 'SELECT \'{"a":1}\'::jsonb ?? \'a\' AS x, ? AS y', ['p']);
    probe($pdo, 'doubled question without a binding', 'SELECT \'{"a":1}\'::jsonb ?? \'a\' AS x', []);
    probe($pdo, 'line comment', "SELECT 1 AS x -- ? :v\n, ? AS y", ['p']);
    probe($pdo, 'nested comment', 'SELECT 1 AS x /* outer /* ? :v */ end */ , ? AS y', ['p']);
    probe($pdo, 'double-quoted identifier', 'SELECT 1 AS "a""?:v", ? AS y', ['p']);
    probe($pdo, 'hash operator', 'SELECT 4 # ?::int AS x', [3]);
    probe($pdo, 'unterminated literal', "SELECT ? AS y, 'a?", ['p']);
    if ($emulate) {
        probe($pdo, 'only an unterminated literal', "SELECT 'a?", ['p']);
    }
    $pdo->exec('SET standard_conforming_strings TO off');
    probe($pdo, 'nonstandard session string', "SELECT 'a\\'b' AS x, ? AS y", ['p']);
}
