<?php
$d=new SQLite3(':memory:');
$r=$d->query('SELECT 1 AS a,2.5 AS b,NULL AS c');
var_dump($r->numColumns(),$r->columnType(0));
for($i=-1;
$i<4;
$i++)var_dump($r->columnName($i));
var_dump($r->fetchArray(SQLITE3_NUM));
for($i=-1;
$i<4;
$i++)var_dump($r->columnType($i));
