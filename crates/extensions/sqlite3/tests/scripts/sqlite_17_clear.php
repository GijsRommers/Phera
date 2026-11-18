<?php
$d=new SQLite3(':memory:');
$s=$d->prepare('SELECT ?');
$s->bindValue(1,42,SQLITE3_INTEGER);
var_dump($s->execute()->fetchArray(SQLITE3_NUM),
$s->clear(),$s->execute()->fetchArray(SQLITE3_NUM));
