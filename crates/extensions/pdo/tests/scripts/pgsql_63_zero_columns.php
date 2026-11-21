<?php
// Oracle: PHP 8.5.12. A PostgreSQL DataRow may have no fields.
$pdo = new PDO('pgsql:host=127.0.0.1;port=54329;dbname=phera_test', 'phera', 'phera');
var_dump($pdo->query('SELECT')->fetchAll());
var_dump($pdo->query('SELECT FROM generate_series(1,2)')->fetchAll(PDO::FETCH_ASSOC));

$pdo->setAttribute(PDO::ATTR_PREFETCH, false);
$statement = $pdo->query('SELECT FROM generate_series(1,2)');
var_dump($statement->columnCount(), $statement->rowCount());
var_dump($statement->fetch(PDO::FETCH_ASSOC), $statement->fetchAll(PDO::FETCH_ASSOC));
