<?php
$d=new SQLite3(':memory:');
$d->exec('CREATE TABLE a(id PRIMARY KEY,v);INSERT INTO a ' .
    'VALUES(1,2);INSERT INTO a VALUES(1,3) ON CONFLICT(id) DO UPDATE ' .
    'SET v=excluded.v');
var_dump($d->querySingle('SELECT v FROM a'));
