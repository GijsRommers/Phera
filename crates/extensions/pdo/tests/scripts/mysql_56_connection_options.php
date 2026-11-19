<?php
$dsn = 'mysql:host=127.0.0.1;port=33069;dbname=phera_test';
$pdo = new PDO($dsn, 'phera', 'phera', [
    PDO::ATTR_TIMEOUT => 5,
    PDO::ATTR_AUTOCOMMIT => false,
    Pdo\Mysql::ATTR_INIT_COMMAND => 'SET @phera_pdo_init = 42',
    Pdo\Mysql::ATTR_COMPRESS => true,
    Pdo\Mysql::ATTR_FOUND_ROWS => true,
    Pdo\Mysql::ATTR_IGNORE_SPACE => true,
    Pdo\Mysql::ATTR_MULTI_STATEMENTS => false,
]);
var_dump($pdo->getAttribute(PDO::ATTR_AUTOCOMMIT));
var_dump($pdo->query('SELECT @phera_pdo_init')->fetchColumn());
$pdo->setAttribute(PDO::ATTR_AUTOCOMMIT, true);
var_dump($pdo->getAttribute(PDO::ATTR_AUTOCOMMIT));
var_dump($pdo->query('SELECT @@autocommit')->fetchColumn());
$pdo->exec('CREATE TEMPORARY TABLE pdo_option_probe (id int, value int)');
$pdo->exec('INSERT INTO pdo_option_probe VALUES (1, 4)');
var_dump($pdo->query('SELECT COUNT (id) FROM pdo_option_probe')->fetchColumn());
var_dump($pdo->exec('UPDATE pdo_option_probe SET value = 4 WHERE id = 1'));
$pdo->setAttribute(PDO::ATTR_FETCH_TABLE_NAMES, true);
var_dump($pdo->query('SELECT p.value AS picked FROM pdo_option_probe p')->fetch(PDO::FETCH_ASSOC));
$pdo->setAttribute(PDO::ATTR_DEFAULT_STR_PARAM, PDO::PARAM_STR_NATL);
var_dump($pdo->quote("it's"), $pdo->quote("it's", PDO::PARAM_STR | PDO::PARAM_STR_CHAR));
try {
    $pdo->exec('SELECT 1; SELECT 2');
} catch (PDOException $e) {
    echo $e->getCode(), "\n";
}
