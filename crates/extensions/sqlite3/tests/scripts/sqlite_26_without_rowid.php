<?php
$d=new SQLite3(':memory:');
$d->exec("CREATE TABLE a(id TEXT PRIMARY KEY) WITHOUT ROWID;INSERT INTO a " .
    "VALUES('a')");
var_dump($d->lastInsertRowID(),$d->querySingle('SELECT id FROM a'));
