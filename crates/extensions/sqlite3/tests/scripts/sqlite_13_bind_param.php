<?php
$d=new SQLite3(':memory:');
$s=$d->prepare('SELECT ?,typeof(?)');
$v=1;
var_dump($s->bindParam(1,$v,SQLITE3_INTEGER),
$s->bindParam(2,$v,SQLITE3_INTEGER));
$v=42;
var_dump($s->execute()->fetchArray(SQLITE3_NUM));
$v=7;
var_dump($s->execute()->fetchArray(SQLITE3_NUM));
