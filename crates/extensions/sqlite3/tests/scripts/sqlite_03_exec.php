<?php
$d=new SQLite3(':memory:');
var_dump($d->exec('CREATE TABLE a(x);INSERT INTO a VALUES(1),(2)'));
var_dump($d->changes(),$d->lastInsertRowID(),
$d->querySingle('SELECT count(*) FROM a'));
