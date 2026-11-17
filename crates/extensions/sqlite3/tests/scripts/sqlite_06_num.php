<?php
$d=new SQLite3(':memory:');
$r=$d->query('SELECT 1 AS a,2 AS b');
var_dump($r->fetchArray(SQLITE3_NUM),
$r->fetchArray(SQLITE3_NUM));
