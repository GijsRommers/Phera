<?php
$d=new SQLite3(':memory:');
$d->exec('CREATE TABLE a(id INTEGER PRIMARY KEY,x);INSERT INTO a(x) VALUES(1),(2)');
var_dump($d->lastInsertRowID(),$d->changes());
$d->exec('UPDATE a SET x=5');
var_dump($d->changes());
