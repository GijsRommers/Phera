<?php
$pdo = new PDO('pgsql:host=127.0.0.1;port=54329;dbname=phera_test', 'phera', 'phera', [
    PDO::ATTR_PREFETCH => false,
    PDO::ATTR_EMULATE_PREPARES => false,
]);
foreach (['server info', 'quote'] as $operation) {
    $statement = $pdo->query('SELECT n FROM generate_series(1, 5000) AS series(n)');
    var_dump($statement->fetchColumn());
    if ($operation === 'server info') {
        $info = $pdo->getAttribute(PDO::ATTR_SERVER_INFO);
        echo preg_replace('/PID: [0-9]+;/', 'PID: <pid>;', $info), "\n";
    } else {
        var_dump($pdo->quote("don't drain the rows"));
    }
    var_dump($statement->fetchColumn(), count($statement->fetchAll(PDO::FETCH_COLUMN)));
}
