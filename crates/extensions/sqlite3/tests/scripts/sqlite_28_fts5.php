<?php
$d=new SQLite3(':memory:');
$d->exec("CREATE VIRTUAL TABLE docs USING fts5(body);INSERT INTO docs " .
    "VALUES('hello sqlite')");
var_dump($d->querySingle("SELECT body FROM docs WHERE docs MATCH 'sqlite'"));
