<?php
$d=new SQLite3(':memory:');
$d->exec('CREATE TABLE a(id INTEGER PRIMARY KEY,v)');
$r=$d->query('INSERT INTO a(v) VALUES(7) RETURNING id');
var_dump($r->fetchArray(SQLITE3_NUM),
$r->fetchArray(SQLITE3_NUM),$d->querySingle('SELECT count(*) FROM a'));
