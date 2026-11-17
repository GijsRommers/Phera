<?php
$d=new SQLite3(':memory:');
$d->exec("ATTACH ':memory:' AS extra;CREATE TABLE extra.a(x);INSERT INTO " .
    "extra.a VALUES(42)");
var_dump($d->querySingle('SELECT x FROM extra.a'));
$d->exec('DETACH extra');
