<?php
$d=new SQLite3(':memory:');
$r=$d->query('SELECT 1 AS a UNION ALL SELECT 2');
var_dump($r->fetchAll(SQLITE3_ASSOC));
