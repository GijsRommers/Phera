<?php
$p = new PDO('pgsql:host=127.0.0.1;port=54329;dbname=phera_test', 'phera', 'phera');
$p->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_SILENT);
$p->exec('CREATE TEMP TABLE parameter_effects(id INT)');
foreach ([[], [1], [1=>2]] as $parameters) {
    $p->beginTransaction();
    $p->exec('INSERT INTO parameter_effects VALUES (1)');
    $statement = $p->prepare('SELECT ? AS a, ? AS b');
    var_dump($statement->execute($parameters));
    var_dump($statement->errorInfo());
    var_dump($p->inTransaction());
    var_dump($p->exec('INSERT INTO parameter_effects VALUES (2)'));
    var_dump($p->errorCode());
    var_dump($p->commit());
    var_dump($p->query('SELECT count(*) FROM parameter_effects')->fetchColumn());
}
