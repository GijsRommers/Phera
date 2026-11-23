<?php
// Oracle: PHP 8.5.12. Buffered emulation returns the last result and a
// non-prefetched fetch keeps the first result's column names and values.
$pdo = new PDO('pgsql:host=127.0.0.1;port=54329;dbname=phera_test', 'phera', 'phera', [
    PDO::ATTR_EMULATE_PREPARES => true,
]);
$sql = 'SELECT 1 AS first; SELECT 2 AS last';
var_dump($pdo->query($sql)->fetchAll(PDO::FETCH_ASSOC));
var_dump($pdo->query("SELECT 1 AS first; SET application_name TO 'pdo_emulated'")->fetchAll(PDO::FETCH_ASSOC));
$pdo->setAttribute(PDO::ATTR_PREFETCH, false);
var_dump($pdo->query($sql)->fetchAll(PDO::FETCH_ASSOC));
var_dump($pdo->query('SELECT 1 AS first; SELECT 1/0')->fetchAll(PDO::FETCH_ASSOC));
var_dump($pdo->query('SELECT 3')->fetchColumn());
